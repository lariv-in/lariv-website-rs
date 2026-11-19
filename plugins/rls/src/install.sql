-- Row-level security keyed by company membership.
-- The serving role is not the table owner, so these policies apply to it.
-- Admin / migration / seed / serve-startup sessions set app.rls_bypass and are
-- treated like a superuser for DDL/DML (see rls_is_bypass).

CREATE OR REPLACE FUNCTION rls_current_user_id()
RETURNS bigint
LANGUAGE plpgsql
VOLATILE
AS $fn$
BEGIN
    RETURN NULLIF(current_setting('app.user_id', true), '')::bigint;
EXCEPTION
    WHEN OTHERS THEN
        RETURN NULL;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_current_company_id()
RETURNS bigint
LANGUAGE plpgsql
VOLATILE
AS $fn$
BEGIN
    RETURN NULLIF(current_setting('app.company_id', true), '')::bigint;
EXCEPTION
    WHEN OTHERS THEN
        RETURN NULL;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_is_superuser()
RETURNS boolean
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    uid bigint := rls_current_user_id();
BEGIN
    IF uid IS NULL THEN
        RETURN false;
    END IF;
    RETURN EXISTS (
        SELECT 1 FROM users u WHERE u.id = uid AND u.role = 'superuser'
    );
END
$fn$;

-- True for the admin/migration role, or when the pool hook stamped app.rls_bypass
-- (serve-startup / seed while the startup actor is active).
CREATE OR REPLACE FUNCTION rls_is_bypass()
RETURNS boolean
LANGUAGE plpgsql
STABLE
AS $fn$
BEGIN
    IF current_user IS DISTINCT FROM 'lariv_website_app' THEN
        RETURN true;
    END IF;
    BEGIN
        -- An empty setting is not a bypass. NULL IN (...) is NULL, and
        -- NOT NULL skips the non-superuser company fallback.
        RETURN COALESCE(
            NULLIF(current_setting('app.rls_bypass', true), '') IN ('1', 'true', 'on'),
            false
        );
    EXCEPTION
        WHEN OTHERS THEN
            RETURN false;
    END;
END
$fn$;

-- Prefs first, then an owned membership, then any membership.
-- Superusers and other users share this fallback. A NULL preference used to
-- leave a superuser with no company, which hid company-owned invoices and
-- quotations after logout and login. A superuser with no membership still
-- resolves to NULL (unassigned rows).
CREATE OR REPLACE FUNCTION rls_resolve_active_company()
RETURNS bigint
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    uid bigint := rls_current_user_id();
    preferred bigint;
    cid bigint;
BEGIN
    IF uid IS NULL THEN
        RETURN NULL;
    END IF;
    SELECT p.active_company_id
    INTO preferred
    FROM company_user_prefs p
    WHERE p.user_id = uid;
    IF preferred IS NOT NULL AND EXISTS (
        SELECT 1 FROM company_memberships m
        WHERE m.user_id = uid AND m.company_id = preferred
    ) THEN
        RETURN preferred;
    END IF;
    SELECT m.company_id
    INTO cid
    FROM company_memberships m
    WHERE m.user_id = uid
    ORDER BY m.owned_by DESC, m.id
    LIMIT 1;
    RETURN cid;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_is_company_member(cid bigint)
RETURNS boolean
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
BEGIN
    IF cid IS NULL OR rls_current_user_id() IS NULL THEN
        RETURN false;
    END IF;
    RETURN EXISTS (
        SELECT 1
        FROM company_memberships m
        WHERE m.company_id = cid
          AND m.user_id = rls_current_user_id()
    );
END
$fn$;

CREATE OR REPLACE FUNCTION rls_is_company_owner(cid bigint)
RETURNS boolean
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
BEGIN
    IF cid IS NULL OR rls_current_user_id() IS NULL THEN
        RETURN false;
    END IF;
    RETURN EXISTS (
        SELECT 1
        FROM company_memberships m
        WHERE m.company_id = cid
          AND m.user_id = rls_current_user_id()
          AND m.owned_by
    );
END
$fn$;

CREATE OR REPLACE FUNCTION rls_company_has_members(cid bigint)
RETURNS boolean
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
BEGIN
    IF cid IS NULL THEN
        RETURN false;
    END IF;
    RETURN EXISTS (
        SELECT 1 FROM company_memberships m WHERE m.company_id = cid
    );
END
$fn$;

-- TG_ARGV[0] owners table
-- TG_ARGV[1] comma-separated owners columns (record ids)
-- TG_ARGV[2] comma-separated source columns on NEW (json keys)
CREATE OR REPLACE FUNCTION rls_own_new_row() RETURNS trigger
LANGUAGE plpgsql AS $fn$
DECLARE
    cid bigint;
    owner_cols text[] := string_to_array(TG_ARGV[1], ',');
    source_cols text[] := string_to_array(TG_ARGV[2], ',');
    col text;
    src text;
    collist text := '';
    vallist text := '';
    i int := 1;
BEGIN
    BEGIN
        cid := rls_current_company_id();
    EXCEPTION
        WHEN OTHERS THEN
            cid := NULL;
    END;
    IF cid IS NULL AND NOT rls_is_bypass() THEN
        -- Use a membership when the session company was not stamped.
        cid := rls_resolve_active_company();
        IF cid IS NOT NULL THEN
            PERFORM set_config('app.company_id', cid::text, true);
        END IF;
    END IF;
    IF cid IS NULL THEN
        -- Migration / seed / admin: leave unowned; rls_backfill assigns later.
        -- filesystem_preferences still inserts: a signed-in user with no company
        -- gets an unassigned root-access row so the filesystem page can open.
        -- Company members are stamped above from their membership.
        IF rls_is_bypass() THEN
            RETURN NEW;
        END IF;
        -- filesystem_preferences may insert an unassigned root with no user.
        -- Other tables still need a company.
        IF TG_TABLE_NAME IS DISTINCT FROM 'filesystem_preferences' THEN
            RAISE EXCEPTION 'rls: active company required to insert into %', TG_TABLE_NAME;
        END IF;
    END IF;
    IF cid IS NOT NULL
       AND NOT rls_is_company_member(cid)
       AND NOT rls_is_bypass()
       AND NOT rls_is_superuser()
    THEN
        RAISE EXCEPTION 'rls: not a member of the active company';
    END IF;
    FOREACH col IN ARRAY owner_cols LOOP
        src := source_cols[i];
        IF collist <> '' THEN
            collist := collist || ', ';
            vallist := vallist || ', ';
        END IF;
        collist := collist || format('%I', col);
        vallist := vallist || format('($2 ->> %L)::bigint', src);
        i := i + 1;
    END LOOP;
    EXECUTE format(
        'INSERT INTO %I (%s, company_id) SELECT %s, $1 ON CONFLICT DO NOTHING',
        TG_ARGV[0],
        collist,
        vallist
    )
    USING cid, to_jsonb(NEW);
    RETURN NEW;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_unique_per_owner() RETURNS trigger
LANGUAGE plpgsql AS $fn$
DECLARE
    owners text := TG_ARGV[0];
    fk text := TG_ARGV[1];
    cols text[] := string_to_array(TG_ARGV[2], ',');
    cid bigint;
    col text;
    pred text := 'TRUE';
    clash boolean;
BEGIN
    BEGIN
        cid := rls_current_company_id();
    EXCEPTION
        WHEN OTHERS THEN
            RETURN NEW;
    END;
    IF cid IS NULL THEN
        RETURN NEW;
    END IF;
    FOREACH col IN ARRAY cols LOOP
        pred := pred || format(' AND t.%I::text = ($1 ->> %L)', col, col);
    END LOOP;
    EXECUTE format(
        'SELECT EXISTS (
            SELECT 1 FROM %I t
            JOIN %I o ON o.%I = t.id
            WHERE o.company_id = $2
              AND t.id IS DISTINCT FROM $3
              AND %s
        )',
        TG_TABLE_NAME,
        owners,
        fk,
        pred
    )
    INTO clash
    USING to_jsonb(NEW), cid, NEW.id;
    IF clash THEN
        RAISE EXCEPTION 'duplicate % for this company on %', TG_ARGV[2], TG_TABLE_NAME
            USING ERRCODE = 'unique_violation';
    END IF;
    RETURN NEW;
END
$fn$;

-- VOLATILE so a row inserted earlier in the same command (the owner trigger)
-- is visible to RETURNING. A plain EXISTS in the policy uses the statement snapshot.
CREATE OR REPLACE FUNCTION rls_row_visible(owners regclass, keys bigint[])
RETURNS boolean
LANGUAGE plpgsql
VOLATILE
AS $fn$
DECLARE
    cols text[];
    col text;
    pred text := 'company_id = rls_current_company_id()';
    i int := 1;
    ok boolean;
BEGIN
    IF rls_is_bypass() THEN
        RETURN true;
    END IF;
    IF rls_current_company_id() IS NULL
       OR rls_current_user_id() IS NULL
       OR keys IS NULL
       OR NOT rls_is_company_member(rls_current_company_id())
    THEN
        RETURN false;
    END IF;
    SELECT array_agg(a.attname ORDER BY a.attnum)
    INTO cols
    FROM pg_attribute a
    WHERE a.attrelid = owners
      AND a.attnum > 0
      AND NOT a.attisdropped
      AND a.attname <> 'company_id';
    IF cols IS NULL THEN
        RETURN false;
    END IF;
    FOREACH col IN ARRAY cols LOOP
        pred := pred || format(' AND %I = $1[%s]', col, i);
        i := i + 1;
    END LOOP;
    EXECUTE format('SELECT EXISTS (SELECT 1 FROM %s WHERE %s)', owners, pred)
        INTO ok
        USING keys;
    RETURN COALESCE(ok, false);
END
$fn$;

-- Optional company: unassigned (NULL) rows are superuser-only; company rows
-- follow the active company like the required-company path.
CREATE OR REPLACE FUNCTION rls_row_visible_optional_company(owners regclass, keys bigint[])
RETURNS boolean
LANGUAGE plpgsql
VOLATILE
AS $fn$
DECLARE
    cols text[];
    col text;
    pred text := '(
        (company_id IS NULL AND rls_is_superuser())
        OR (
            company_id IS NOT NULL
            AND company_id = rls_current_company_id()
            AND rls_is_company_member(company_id)
        )
    )';
    i int := 1;
    ok boolean;
BEGIN
    IF rls_is_bypass() THEN
        RETURN true;
    END IF;
    IF rls_current_user_id() IS NULL OR keys IS NULL THEN
        RETURN false;
    END IF;
    SELECT array_agg(a.attname ORDER BY a.attnum)
    INTO cols
    FROM pg_attribute a
    WHERE a.attrelid = owners
      AND a.attnum > 0
      AND NOT a.attisdropped
      AND a.attname <> 'company_id';
    IF cols IS NULL THEN
        RETURN false;
    END IF;
    FOREACH col IN ARRAY cols LOOP
        pred := pred || format(' AND %I = $1[%s]', col, i);
        i := i + 1;
    END LOOP;
    EXECUTE format('SELECT EXISTS (SELECT 1 FROM %s WHERE %s)', owners, pred)
        INTO ok
        USING keys;
    RETURN COALESCE(ok, false);
END
$fn$;

CREATE OR REPLACE FUNCTION rls_own_new_row_optional_company() RETURNS trigger
LANGUAGE plpgsql AS $fn$
DECLARE
    cid bigint;
    owner_cols text[] := string_to_array(TG_ARGV[1], ',');
    source_cols text[] := string_to_array(TG_ARGV[2], ',');
    col text;
    src text;
    collist text := '';
    vallist text := '';
    i int := 1;
BEGIN
    BEGIN
        cid := rls_current_company_id();
    EXCEPTION
        WHEN OTHERS THEN
            cid := NULL;
    END;
    -- Audit logs always stamp whatever company is present (including NULL).
    -- filesystem_nodes still require a company for non-superusers (unless bypass).
    IF cid IS NULL THEN
        IF TG_TABLE_NAME <> 'logs'
            AND NOT rls_is_superuser()
            AND NOT rls_is_bypass()
        THEN
            RAISE EXCEPTION 'rls: active company required to insert into %', TG_TABLE_NAME;
        END IF;
    ELSIF NOT rls_is_company_member(cid)
        AND TG_TABLE_NAME <> 'logs'
        AND NOT rls_is_bypass()
    THEN
        RAISE EXCEPTION 'rls: not a member of the active company';
    END IF;
    FOREACH col IN ARRAY owner_cols LOOP
        src := source_cols[i];
        IF collist <> '' THEN
            collist := collist || ', ';
            vallist := vallist || ', ';
        END IF;
        collist := collist || format('%I', col);
        vallist := vallist || format('($2 ->> %L)::bigint', src);
        i := i + 1;
    END LOOP;
    EXECUTE format(
        'INSERT INTO %I (%s, company_id) SELECT %s, $1 ON CONFLICT DO NOTHING',
        TG_ARGV[0],
        collist,
        vallist
    )
    USING cid, to_jsonb(NEW);
    RETURN NEW;
END
$fn$;

-- 39 characters, so it cannot collide with a 63-byte identifier limit.
CREATE OR REPLACE FUNCTION rls_owners_name(table_name text)
RETURNS text
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT 'rlsown_' || md5(table_name)
$$;

CREATE OR REPLACE FUNCTION rls_owner_fk(owners regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT a.attname
    FROM pg_attribute a
    WHERE a.attrelid = owners
      AND a.attnum > 0
      AND NOT a.attisdropped
      AND a.attname <> 'company_id'
    ORDER BY a.attnum
    LIMIT 1
$$;

CREATE OR REPLACE FUNCTION rls_membership_keep_owner()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    cid bigint;
    remaining boolean;
BEGIN
    cid := COALESCE(OLD.company_id, NEW.company_id);
    IF TG_OP = 'DELETE' THEN
        IF OLD.owned_by THEN
            SELECT EXISTS (
                SELECT 1 FROM company_memberships m
                WHERE m.company_id = cid
                  AND m.owned_by
                  AND m.id IS DISTINCT FROM OLD.id
            ) INTO remaining;
            IF NOT remaining THEN
                RAISE EXCEPTION 'company % must keep at least one owner', cid;
            END IF;
        END IF;
        RETURN OLD;
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.owned_by AND NOT NEW.owned_by THEN
        SELECT EXISTS (
            SELECT 1 FROM company_memberships m
            WHERE m.company_id = cid
              AND m.owned_by
              AND m.id IS DISTINCT FROM NEW.id
        ) INTO remaining;
        IF NOT remaining THEN
            RAISE EXCEPTION 'company % must keep at least one owner', cid;
        END IF;
    END IF;
    RETURN NEW;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_find_user_by_email(p_email text)
RETURNS SETOF users
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT * FROM users WHERE email = p_email LIMIT 1
$$;

CREATE OR REPLACE FUNCTION rls_signup_user(
    p_name text,
    p_email text,
    p_phone text,
    p_password bytea,
    p_salt bytea,
    p_timezone text
)
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    new_id bigint;
BEGIN
    INSERT INTO users (
        created_at,
        updated_at,
        name,
        email,
        phone,
        role,
        password,
        password_salt,
        timezone,
        last_login_time
    )
    VALUES (
        now(),
        now(),
        p_name,
        p_email,
        p_phone,
        'unassigned',
        p_password,
        p_salt,
        COALESCE(NULLIF(p_timezone, ''), 'Asia/Kolkata'),
        NULL
    )
    RETURNING id INTO new_id;
    RETURN new_id;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_backfill(admin_id bigint)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    rec record;
    owners text;
    base text;
    cols text[];
    col text;
    collist text;
    sellist text;
    pred text;
    company_id bigint;
BEGIN
    IF admin_id IS NULL THEN
        RETURN;
    END IF;
    SELECT m.company_id
    INTO company_id
    FROM company_memberships m
    WHERE m.user_id = admin_id
      AND m.owned_by
    ORDER BY m.id
    LIMIT 1;
    IF company_id IS NULL THEN
        INSERT INTO companies (created_at, updated_at, name)
        VALUES (now(), now(), 'Admin company')
        RETURNING id INTO company_id;
        INSERT INTO company_memberships (
            created_at, updated_at, user_id, company_id, owned_by
        )
        VALUES (now(), now(), admin_id, company_id, true);
    END IF;
    FOR rec IN
        SELECT base_table, owners_table
        FROM rls_owner_tables
    LOOP
        owners := rec.owners_table;
        base := rec.base_table;
        SELECT array_agg(a.attname ORDER BY a.attnum)
        INTO cols
        FROM pg_attribute a
        WHERE a.attrelid = to_regclass(format('public.%I', owners))
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND a.attname <> 'company_id';
        collist := '';
        sellist := '';
        pred := 'TRUE';
        FOREACH col IN ARRAY cols LOOP
            IF collist <> '' THEN
                collist := collist || ', ';
                sellist := sellist || ', ';
            END IF;
            collist := collist || format('%I', col);
            IF col = 'record_id' THEN
                sellist := sellist || 't.id';
                pred := pred || format(' AND o.%I = t.id', col);
            ELSE
                sellist := sellist || format('t.%I', col);
                pred := pred || format(' AND o.%I = t.%I', col, col);
            END IF;
        END LOOP;
        EXECUTE format(
            'INSERT INTO %I (%s, company_id)
             SELECT %s, $1 FROM %I t
             WHERE NOT EXISTS (SELECT 1 FROM %I o WHERE %s)
             ON CONFLICT DO NOTHING',
            owners,
            collist,
            sellist,
            base,
            owners,
            pred
        )
        USING company_id;
    END LOOP;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_admin_company_id(admin_id bigint)
RETURNS bigint
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    company_id bigint;
BEGIN
    SELECT m.company_id
    INTO company_id
    FROM company_memberships m
    WHERE m.user_id = admin_id
      AND m.owned_by
    ORDER BY m.id
    LIMIT 1;
    RETURN company_id;
END
$fn$;

-- Unassigned root access (no active company) is visible with no signed-in user,
-- so the public site and a first visit can read it instead of inserting blindly.
-- An active company sees only its own row. Superusers see every row.
CREATE OR REPLACE FUNCTION rls_filesystem_pref_visible(owners regclass, key bigint)
RETURNS boolean
LANGUAGE plpgsql
VOLATILE
AS $fn$
DECLARE
    ok boolean;
BEGIN
    IF rls_is_bypass() OR rls_is_superuser() THEN
        RETURN true;
    END IF;
    IF key IS NULL THEN
        RETURN false;
    END IF;
    IF rls_current_user_id() IS NULL OR rls_current_company_id() IS NULL THEN
        EXECUTE format(
            'SELECT EXISTS (SELECT 1 FROM %s WHERE record_id = $1 AND company_id IS NULL)',
            owners
        )
            INTO ok
            USING key;
        RETURN COALESCE(ok, false);
    END IF;
    RETURN rls_row_visible(owners, ARRAY[key]);
END
$fn$;

-- A VNode the public website may read with no signed-in user.
-- The /website tree is the public content root (pages, static files, themes).
-- Active route pages, their references, and custom-theme files are visible
-- even when they live outside that tree, and so are the directories above
-- them, so path lookup works. Other vnode trees stay company-scoped.
CREATE OR REPLACE FUNCTION rls_public_website_vnode(node_id bigint)
RETURNS boolean
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    visible boolean := false;
BEGIN
    IF node_id IS NULL OR to_regclass('public.filesystem_nodes') IS NULL THEN
        RETURN false;
    END IF;
    IF to_regclass('public.db_routes') IS NOT NULL THEN
        EXECUTE $sql$
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id, 0 AS depth
                FROM filesystem_nodes
                WHERE id = $1
                UNION ALL
                SELECT n.id, n.parent_id, a.depth + 1
                FROM filesystem_nodes n
                JOIN ancestors a ON n.id = a.parent_id
                WHERE a.depth < 64
            )
            SELECT EXISTS (
                SELECT 1
                FROM ancestors a
                JOIN db_routes r ON r.page_id = a.id
                WHERE r.is_active
            )
        $sql$
            INTO visible
            USING node_id;
        IF visible THEN
            RETURN true;
        END IF;
    END IF;
    IF to_regclass('public.db_routes') IS NOT NULL
       AND to_regclass('public.p_website_route_references') IS NOT NULL
    THEN
        EXECUTE $sql$
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id, 0 AS depth
                FROM filesystem_nodes
                WHERE id = $1
                UNION ALL
                SELECT n.id, n.parent_id, a.depth + 1
                FROM filesystem_nodes n
                JOIN ancestors a ON n.id = a.parent_id
                WHERE a.depth < 64
            )
            SELECT EXISTS (
                SELECT 1
                FROM ancestors a
                JOIN p_website_route_references ref ON ref.v_node_id = a.id
                JOIN db_routes r ON r.id = ref.db_route_id
                WHERE r.is_active
            )
        $sql$
            INTO visible
            USING node_id;
        IF visible THEN
            RETURN true;
        END IF;
    END IF;
    IF to_regclass('public.website_preferences') IS NOT NULL THEN
        EXECUTE $sql$
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id, 0 AS depth
                FROM filesystem_nodes
                WHERE id = $1
                UNION ALL
                SELECT n.id, n.parent_id, a.depth + 1
                FROM filesystem_nodes n
                JOIN ancestors a ON n.id = a.parent_id
                WHERE a.depth < 64
            )
            SELECT EXISTS (
                SELECT 1
                FROM ancestors a
                JOIN website_preferences p
                  ON p.custom_theme_css_vnode_id = a.id
                  OR p.custom_theme_js_vnode_id = a.id
            )
        $sql$
            INTO visible
            USING node_id;
        IF visible THEN
            RETURN true;
        END IF;
    END IF;
    EXECUTE $sql$
        WITH RECURSIVE ancestors AS (
            SELECT id, parent_id, name, 0 AS depth
            FROM filesystem_nodes
            WHERE id = $1
            UNION ALL
            SELECT n.id, n.parent_id, n.name, a.depth + 1
            FROM filesystem_nodes n
            JOIN ancestors a ON n.id = a.parent_id
            WHERE a.depth < 64
        )
        SELECT EXISTS (
            SELECT 1
            FROM ancestors dir
            WHERE dir.name = 'website'
              AND dir.parent_id IS NULL
        )
    $sql$
        INTO visible
        USING node_id;
    RETURN COALESCE(visible, false);
END
$fn$;

-- Website catalog tables are read by anonymous visitors. Company ownership
-- does not apply. Writes still require a signed-in member, a superuser, or bypass.
CREATE OR REPLACE FUNCTION rls_install_public_website_table(target text)
RETURNS void
LANGUAGE plpgsql
AS $fn$
DECLARE
    owners text := rls_owners_name(target);
    trg record;
    select_using text;
    write_check text := 'rls_is_bypass() OR rls_is_superuser() OR (rls_current_user_id() IS NOT NULL AND rls_current_company_id() IS NOT NULL AND rls_is_company_member(rls_current_company_id()))';
BEGIN
    IF to_regclass(format('public.%I', target)) IS NULL THEN
        RETURN;
    END IF;
    DELETE FROM rls_owner_tables WHERE base_table = target;
    EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', owners);
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', target);
    EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', target);
    EXECUTE format('DROP POLICY IF EXISTS rls_update ON %I', target);
    EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', target);
    EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', target);
    FOR trg IN
        SELECT t.tgname
        FROM pg_trigger t
        JOIN pg_proc p ON p.oid = t.tgfoid
        WHERE t.tgrelid = to_regclass(format('public.%I', target))
          AND NOT t.tgisinternal
          AND p.proname IN (
              'rls_own_new_row',
              'rls_own_new_row_optional_company',
              'rls_unique_per_owner'
          )
    LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', trg.tgname, target);
    END LOOP;
    IF target = 'db_routes' THEN
        select_using := 'is_active OR ' || write_check;
    ELSIF target = 'p_website_route_references' THEN
        select_using := 'rls_is_bypass() OR rls_is_superuser() OR EXISTS (SELECT 1 FROM db_routes r WHERE r.id = db_route_id AND r.is_active) OR (rls_current_user_id() IS NOT NULL AND rls_current_company_id() IS NOT NULL AND rls_is_company_member(rls_current_company_id()))';
    ELSE
        select_using := 'true';
    END IF;
    EXECUTE format(
        'CREATE POLICY rls_select ON %I FOR SELECT USING (%s)',
        target,
        select_using
    );
    EXECUTE format(
        'CREATE POLICY rls_update ON %I FOR UPDATE USING (%s) WITH CHECK (%s)',
        target,
        write_check,
        write_check
    );
    EXECUTE format(
        'CREATE POLICY rls_delete ON %I FOR DELETE USING (%s)',
        target,
        write_check
    );
    EXECUTE format(
        'CREATE POLICY rls_insert ON %I FOR INSERT WITH CHECK (%s)',
        target,
        write_check
    );
END
$fn$;

-- Root access is per company, and also opens with no company selected.
-- Called at the end of rls_install_policies so it wins over the generic rules.
CREATE OR REPLACE FUNCTION rls_install_filesystem_preferences()
RETURNS void
LANGUAGE plpgsql
AS $fn$
DECLARE
    owners text := rls_owners_name('filesystem_preferences');
    con_name text;
BEGIN
    IF to_regclass('public.filesystem_preferences') IS NULL THEN
        RETURN;
    END IF;
    INSERT INTO rls_owner_tables (base_table, owners_table)
    VALUES ('filesystem_preferences', owners)
    ON CONFLICT (base_table) DO UPDATE SET owners_table = EXCLUDED.owners_table;
    IF to_regclass(format('public.%I', owners)) IS NULL THEN
        EXECUTE format(
            'CREATE TABLE %I (
                record_id bigint NOT NULL REFERENCES filesystem_preferences (id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
                company_id bigint NULL REFERENCES companies (id) ON DELETE CASCADE,
                PRIMARY KEY (record_id)
            )',
            owners
        );
    ELSE
        FOR con_name IN
            SELECT con.conname
            FROM pg_constraint con
            WHERE con.conrelid = to_regclass(format('public.%I', owners))
              AND con.contype = 'p'
              AND (SELECT count(*) FROM unnest(con.conkey) AS k) IS DISTINCT FROM 1
        LOOP
            EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', owners, con_name);
        END LOOP;
        EXECUTE format('ALTER TABLE %I ALTER COLUMN company_id DROP NOT NULL', owners);
        IF NOT EXISTS (
            SELECT 1
            FROM pg_constraint con
            WHERE con.conrelid = to_regclass(format('public.%I', owners))
              AND con.contype = 'p'
        ) THEN
            EXECUTE format('ALTER TABLE %I ADD PRIMARY KEY (record_id)', owners);
        END IF;
    END IF;
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', owners);
    EXECUTE 'ALTER TABLE filesystem_preferences ENABLE ROW LEVEL SECURITY';
    EXECUTE 'DROP POLICY IF EXISTS rls_select ON filesystem_preferences';
    EXECUTE 'DROP POLICY IF EXISTS rls_update ON filesystem_preferences';
    EXECUTE 'DROP POLICY IF EXISTS rls_delete ON filesystem_preferences';
    EXECUTE 'DROP POLICY IF EXISTS rls_insert ON filesystem_preferences';
    EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', owners);
    EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', owners);
    EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', owners);
    EXECUTE format(
        'CREATE POLICY rls_select ON filesystem_preferences FOR SELECT USING (rls_filesystem_pref_visible(%L::regclass, id))',
        owners
    );
    EXECUTE format(
        'CREATE POLICY rls_update ON filesystem_preferences FOR UPDATE USING (rls_filesystem_pref_visible(%L::regclass, id)) WITH CHECK (rls_filesystem_pref_visible(%L::regclass, id))',
        owners,
        owners
    );
    EXECUTE format(
        'CREATE POLICY rls_delete ON filesystem_preferences FOR DELETE USING (rls_filesystem_pref_visible(%L::regclass, id))',
        owners
    );
    EXECUTE $pol$
        CREATE POLICY rls_insert ON filesystem_preferences FOR INSERT WITH CHECK (
            rls_is_bypass()
            OR rls_is_superuser()
            OR rls_current_user_id() IS NULL
            OR (
                rls_current_user_id() IS NOT NULL
                AND (
                    rls_current_company_id() IS NULL
                    OR rls_is_company_member(rls_current_company_id())
                )
            )
        )
    $pol$;
    EXECUTE format(
        'CREATE POLICY rls_select ON %I FOR SELECT USING (
            rls_is_bypass()
            OR rls_is_superuser()
            OR (
                company_id IS NULL
                AND rls_current_company_id() IS NULL
            )
            OR (
                company_id IS NOT NULL
                AND company_id = rls_current_company_id()
                AND rls_is_company_member(company_id)
            )
        )',
        owners
    );
    EXECUTE format(
        'CREATE POLICY rls_delete ON %I FOR DELETE USING (
            rls_is_bypass()
            OR rls_is_superuser()
            OR (
                company_id IS NULL
                AND rls_current_user_id() IS NOT NULL
                AND rls_current_company_id() IS NULL
            )
            OR (
                company_id IS NOT NULL
                AND company_id = rls_current_company_id()
                AND rls_is_company_member(company_id)
            )
        )',
        owners
    );
    EXECUTE format(
        'CREATE POLICY rls_insert ON %I FOR INSERT WITH CHECK (
            rls_is_bypass()
            OR rls_is_superuser()
            OR (
                company_id IS NULL
                AND rls_current_company_id() IS NULL
            )
            OR (
                company_id IS NOT NULL
                AND company_id = rls_current_company_id()
                AND rls_is_company_member(company_id)
            )
        )',
        owners
    );
    EXECUTE 'DROP TRIGGER IF EXISTS rls_own_row ON filesystem_preferences';
    EXECUTE format(
        'CREATE TRIGGER rls_own_row BEFORE INSERT ON filesystem_preferences
         FOR EACH ROW EXECUTE FUNCTION rls_own_new_row(%L, %L, %L)',
        owners,
        'record_id',
        'id'
    );
END
$fn$;

CREATE OR REPLACE FUNCTION rls_install_policies()
RETURNS void
LANGUAGE plpgsql AS $fn$
DECLARE
    rec record;
    pk_count int;
    pk_names text[];
    pk_types text[];
    pk_name text;
    pk_type text;
    owners text;
    owner_cols text;
    source_cols text;
    match_pred text;
    fk_cols text;
    fk_list text;
    idx record;
    trg record;
    con_name text;
    col_list text;
BEGIN
    -- Treat policy install / migration DML on this connection as privileged.
    PERFORM set_config('app.rls_bypass', '1', false);

    CREATE TABLE IF NOT EXISTS rls_owner_tables (
        base_table text PRIMARY KEY,
        owners_table text NOT NULL UNIQUE
    );

    -- Drop leftover sidecars from an older naming scheme. Tables this install
    -- still addresses (rls_owners_name of a live base table) stay in place:
    -- startup re-applies this script after every migration pass, and dropping
    -- them would discard each company's chart and currencies. The next backfill
    -- would then hang every unowned copy on the admin company.
    FOR rec IN
        SELECT c.relname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND (
              c.relname LIKE 'rlsown\_%' ESCAPE '\'
              OR c.relname LIKE '%\_owners' ESCAPE '\'
          )
    LOOP
        IF EXISTS (
            SELECT 1
            FROM pg_class base
            JOIN pg_namespace bn ON bn.oid = base.relnamespace
            WHERE bn.nspname = 'public'
              AND base.relkind = 'r'
              AND rls_owners_name(base.relname) = rec.relname
        ) THEN
            CONTINUE;
        END IF;
        EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', rec.relname);
    END LOOP;
    DELETE FROM rls_owner_tables;

    IF to_regclass('public.company_memberships') IS NOT NULL THEN
        EXECUTE 'DROP TRIGGER IF EXISTS rls_membership_keep_owner ON company_memberships';
        EXECUTE $trig$
            CREATE TRIGGER rls_membership_keep_owner
            BEFORE UPDATE OR DELETE ON company_memberships
            FOR EACH ROW
            EXECUTE FUNCTION rls_membership_keep_owner()
        $trig$;
    END IF;

    FOR rec IN
        SELECT c.relname AS table_name, c.oid AS table_oid
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND c.relname NOT IN (
              'seaql_migrations',
              'roles',
              'rls_owner_tables',
              'companies',
              'company_memberships',
              'company_user_prefs'
          )
          AND c.relname NOT LIKE 'rlsown\_%' ESCAPE '\'
          AND c.relname NOT LIKE '%\_owners' ESCAPE '\'
        ORDER BY c.relname
    LOOP
        -- Public site catalog. Anonymous visitors read active routes; the
        -- renderer also reads preferences to find theme files.
        IF rec.table_name IN (
            'db_routes',
            'p_website_route_references',
            'website_preferences'
        ) THEN
            PERFORM rls_install_public_website_table(rec.table_name);
            CONTINUE;
        END IF;

        -- Shared IFSC directory. The background sync inserts with no active company.
        IF rec.table_name IN ('banks', 'bank_directory_syncs') THEN
            EXECUTE format('ALTER TABLE %I DISABLE ROW LEVEL SECURITY', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_update ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', rec.table_name);
            FOR trg IN
                SELECT t.tgname
                FROM pg_trigger t
                JOIN pg_proc p ON p.oid = t.tgfoid
                WHERE t.tgrelid = rec.table_oid
                  AND NOT t.tgisinternal
                  AND p.proname IN (
                      'rls_own_new_row',
                      'rls_own_new_row_optional_company',
                      'rls_unique_per_owner'
                  )
            LOOP
                EXECUTE format(
                    'DROP TRIGGER IF EXISTS %I ON %I',
                    trg.tgname,
                    rec.table_name
                );
            END LOOP;
            CONTINUE;
        END IF;

        -- Site-wide API key fallbacks. Superusers edit the row; other roles
        -- cannot read it. Resolution uses the admin connection.
        IF rec.table_name = 'global_preferences' THEN
            EXECUTE 'ALTER TABLE global_preferences ENABLE ROW LEVEL SECURITY';
            EXECUTE 'DROP POLICY IF EXISTS rls_select ON global_preferences';
            EXECUTE 'DROP POLICY IF EXISTS rls_update ON global_preferences';
            EXECUTE 'DROP POLICY IF EXISTS rls_delete ON global_preferences';
            EXECUTE 'DROP POLICY IF EXISTS rls_insert ON global_preferences';
            FOR trg IN
                SELECT t.tgname
                FROM pg_trigger t
                JOIN pg_proc p ON p.oid = t.tgfoid
                WHERE t.tgrelid = rec.table_oid
                  AND NOT t.tgisinternal
                  AND p.proname IN (
                      'rls_own_new_row',
                      'rls_own_new_row_optional_company',
                      'rls_unique_per_owner'
                  )
            LOOP
                EXECUTE format(
                    'DROP TRIGGER IF EXISTS %I ON global_preferences',
                    trg.tgname
                );
            END LOOP;
            EXECUTE 'CREATE POLICY rls_select ON global_preferences FOR SELECT USING (rls_is_bypass() OR rls_is_superuser())';
            EXECUTE 'CREATE POLICY rls_update ON global_preferences FOR UPDATE USING (rls_is_bypass() OR rls_is_superuser()) WITH CHECK (rls_is_bypass() OR rls_is_superuser())';
            EXECUTE 'CREATE POLICY rls_delete ON global_preferences FOR DELETE USING (rls_is_bypass() OR rls_is_superuser())';
            EXECUTE 'CREATE POLICY rls_insert ON global_preferences FOR INSERT WITH CHECK (rls_is_bypass() OR rls_is_superuser())';
            CONTINUE;
        END IF;

        SELECT count(*),
               array_agg(a.attname ORDER BY a.attnum),
               array_agg(format_type(a.atttypid, a.atttypmod) ORDER BY a.attnum)
        INTO pk_count, pk_names, pk_types
        FROM pg_index i
        JOIN pg_attribute a
          ON a.attrelid = i.indrelid
         AND a.attnum = ANY (i.indkey)
        WHERE i.indrelid = rec.table_oid
          AND i.indisprimary;

        IF pk_count IS NULL OR pk_count < 1 THEN
            RAISE EXCEPTION 'rls: table % has no primary key', rec.table_name;
        END IF;
        IF EXISTS (SELECT 1 FROM unnest(pk_types) AS t(typ) WHERE t.typ IS DISTINCT FROM 'bigint') THEN
            RAISE EXCEPTION
                'rls: table % primary key % is not all bigint',
                rec.table_name, pk_names;
        END IF;
        pk_name := pk_names[1];
        pk_type := pk_types[1];

        IF rec.table_name = 'users' THEN
            EXECUTE 'ALTER TABLE users ENABLE ROW LEVEL SECURITY';
            EXECUTE 'DROP POLICY IF EXISTS rls_select ON users';
            EXECUTE 'DROP POLICY IF EXISTS rls_update ON users';
            EXECUTE 'DROP POLICY IF EXISTS rls_insert ON users';
            EXECUTE 'DROP POLICY IF EXISTS rls_delete ON users';
            -- Self, superuser, or anyone who shares a company membership
            -- (so company member lists / assignee labels can resolve names).
            EXECUTE $pol$
                CREATE POLICY rls_select ON users
                FOR SELECT
                USING (
                    id = rls_current_user_id()
                    OR rls_is_superuser()
                    OR EXISTS (
                        SELECT 1
                        FROM company_memberships me
                        JOIN company_memberships them
                          ON them.company_id = me.company_id
                        WHERE me.user_id = rls_current_user_id()
                          AND them.user_id = users.id
                    )
                )
            $pol$;
            EXECUTE $pol$
                CREATE POLICY rls_update ON users
                FOR UPDATE
                USING (id = rls_current_user_id())
                WITH CHECK (id = rls_current_user_id())
            $pol$;
            CONTINUE;
        END IF;

        -- Optional company tenancy: unassigned rows are superuser-only;
        -- company-attached rows follow the active company. Deployment-only.
        -- filesystem_preferences is required-company (same as other *_preferences),
        -- not listed here — each company gets its own root access row, and a
        -- non-superuser inserts it under their membership company.
        -- Assistant tables (single bigint id) are listed so a superuser with no
        -- active company can insert; the owner link stores a NULL company.
        -- Composite-key tables such as llm_assistant_skill_files stay required.
        IF rec.table_name IN ('filesystem_nodes', 'logs')
           OR (
                pk_count = 1
                AND pk_name = 'id'
                AND (
                    rec.table_name = 'skills'
                    OR rec.table_name LIKE 'llm\_assistant\_%' ESCAPE '\'
                )
           )
        THEN
            owners := rls_owners_name(rec.table_name);
            INSERT INTO rls_owner_tables (base_table, owners_table)
            VALUES (rec.table_name, owners)
            ON CONFLICT (base_table) DO UPDATE SET owners_table = EXCLUDED.owners_table;
            EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', owners);
            EXECUTE format(
                'CREATE TABLE %I (
                    record_id bigint NOT NULL REFERENCES %I (id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
                    company_id bigint NULL REFERENCES companies (id) ON DELETE CASCADE,
                    PRIMARY KEY (record_id)
                )',
                owners,
                rec.table_name
            );
            EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', owners);
            EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', rec.table_name);
            EXECUTE format('DROP TRIGGER IF EXISTS rls_own_row ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_update ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', rec.table_name);
            EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', owners);
            EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', owners);
            EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', owners);
            IF rec.table_name = 'filesystem_nodes' THEN
                EXECUTE format(
                    'CREATE POLICY rls_select ON %I FOR SELECT USING (
                        rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[])
                        OR rls_public_website_vnode(id)
                    )',
                    rec.table_name,
                    owners
                );
            ELSE
                EXECUTE format(
                    'CREATE POLICY rls_select ON %I FOR SELECT USING (
                        rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[])
                    )',
                    rec.table_name,
                    owners
                );
            END IF;
            -- A superuser can open a public website file (header.html under
            -- /website) without belonging to the company that owns the row.
            -- The company check then matches zero rows and the save is a no-op.
            IF rec.table_name = 'filesystem_nodes' THEN
                EXECUTE format(
                    'CREATE POLICY rls_update ON %I FOR UPDATE
                     USING (
                        rls_is_bypass()
                        OR rls_is_superuser()
                        OR rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[])
                     )
                     WITH CHECK (
                        rls_is_bypass()
                        OR rls_is_superuser()
                        OR rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[])
                     )',
                    rec.table_name,
                    owners,
                    owners
                );
                EXECUTE format(
                    'CREATE POLICY rls_delete ON %I FOR DELETE USING (
                        rls_is_bypass()
                        OR rls_is_superuser()
                        OR rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[])
                    )',
                    rec.table_name,
                    owners
                );
            ELSE
                EXECUTE format(
                    'CREATE POLICY rls_update ON %I FOR UPDATE
                     USING (rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[]))
                     WITH CHECK (rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[]))',
                    rec.table_name,
                    owners,
                    owners
                );
                EXECUTE format(
                    'CREATE POLICY rls_delete ON %I FOR DELETE USING (
                        rls_row_visible_optional_company(%L::regclass, ARRAY[id]::bigint[])
                    )',
                    rec.table_name,
                    owners
                );
            END IF;
            IF rec.table_name = 'logs' THEN
                -- Audit inserts must always succeed; company on the owner row is optional.
                EXECUTE format(
                    'CREATE POLICY rls_insert ON %I FOR INSERT WITH CHECK (true)',
                    rec.table_name
                );
                EXECUTE format(
                    'CREATE POLICY rls_insert ON %I FOR INSERT WITH CHECK (true)',
                    owners
                );
            ELSE
                EXECUTE format(
                    'CREATE POLICY rls_insert ON %I FOR INSERT WITH CHECK (
                        rls_is_bypass()
                        OR (
                            rls_current_user_id() IS NOT NULL
                            AND (
                                rls_is_superuser()
                                OR (
                                    rls_current_company_id() IS NOT NULL
                                    AND rls_is_company_member(rls_current_company_id())
                                )
                            )
                        )
                    )',
                    rec.table_name
                );
                EXECUTE format(
                    'CREATE POLICY rls_insert ON %I FOR INSERT WITH CHECK (
                        rls_is_bypass()
                        OR (
                            rls_current_user_id() IS NOT NULL
                            AND (
                                (company_id IS NULL AND rls_is_superuser())
                                OR (
                                    company_id IS NOT NULL
                                    AND company_id = rls_current_company_id()
                                    AND rls_is_company_member(company_id)
                                )
                            )
                        )
                    )',
                    owners
                );
            END IF;
            EXECUTE format(
                'CREATE POLICY rls_select ON %I FOR SELECT USING (
                    (company_id IS NULL AND rls_is_superuser())
                    OR (
                        company_id IS NOT NULL
                        AND company_id = rls_current_company_id()
                        AND rls_is_company_member(company_id)
                    )
                )',
                owners
            );
            EXECUTE format(
                'CREATE POLICY rls_delete ON %I FOR DELETE USING (
                    (company_id IS NULL AND rls_is_superuser())
                    OR (
                        company_id IS NOT NULL
                        AND company_id = rls_current_company_id()
                        AND rls_is_company_member(company_id)
                    )
                )',
                owners
            );
            -- Stamp the owner row BEFORE insert. SeaORM inserts with RETURNING,
            -- and Postgres applies the SELECT policy before AFTER triggers, so an
            -- AFTER owner row is invisible and the insert is rejected.
            -- The owners foreign key is deferred, so the link may be written
            -- before the new row is visible.
            EXECUTE format(
                'CREATE TRIGGER rls_own_row BEFORE INSERT ON %I
                 FOR EACH ROW EXECUTE FUNCTION rls_own_new_row_optional_company(%L, %L, %L)',
                rec.table_name,
                owners,
                'record_id',
                'id'
            );
            CONTINUE;
        END IF;

        owners := rls_owners_name(rec.table_name);
        INSERT INTO rls_owner_tables (base_table, owners_table)
        VALUES (rec.table_name, owners)
        ON CONFLICT (base_table) DO UPDATE SET owners_table = EXCLUDED.owners_table;
        IF pk_count = 1 AND pk_name = 'id' THEN
            owner_cols := 'record_id';
            source_cols := 'id';
            fk_cols := 'record_id';
            fk_list := 'record_id';
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS %I (
                    record_id bigint NOT NULL REFERENCES %I (id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
                    company_id bigint NOT NULL REFERENCES companies (id) ON DELETE CASCADE,
                    PRIMARY KEY (record_id, company_id)
                )',
                owners,
                rec.table_name
            );
        ELSE
            owner_cols := array_to_string(pk_names, ',');
            source_cols := owner_cols;
            fk_cols := '';
            fk_list := '';
            FOREACH pk_name IN ARRAY pk_names LOOP
                IF fk_cols <> '' THEN
                    fk_cols := fk_cols || ', ';
                    fk_list := fk_list || ', ';
                END IF;
                fk_cols := fk_cols || format('%I bigint NOT NULL', pk_name);
                fk_list := fk_list || format('%I', pk_name);
            END LOOP;
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS %I (
                    %s,
                    company_id bigint NOT NULL REFERENCES companies (id) ON DELETE CASCADE,
                    PRIMARY KEY (%s, company_id),
                    FOREIGN KEY (%s) REFERENCES %I (%s) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
                )',
                owners,
                fk_cols,
                fk_list,
                fk_list,
                rec.table_name,
                fk_list
            );
        END IF;
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', rec.table_name);
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', owners);
        EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', rec.table_name);
        EXECUTE format('DROP POLICY IF EXISTS rls_update ON %I', rec.table_name);
        EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', rec.table_name);
        EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', rec.table_name);
        EXECUTE format('DROP POLICY IF EXISTS rls_select ON %I', owners);
        EXECUTE format('DROP POLICY IF EXISTS rls_delete ON %I', owners);
        EXECUTE format('DROP POLICY IF EXISTS rls_insert ON %I', owners);

        IF pk_count = 1 AND pk_names[1] = 'id' THEN
            match_pred := format(
                'rls_row_visible(%L::regclass, ARRAY[%I.id])',
                owners,
                rec.table_name
            );
        ELSE
            match_pred := '';
            FOREACH pk_name IN ARRAY pk_names LOOP
                IF match_pred <> '' THEN
                    match_pred := match_pred || ', ';
                END IF;
                match_pred := match_pred || format('%I.%I', rec.table_name, pk_name);
            END LOOP;
            match_pred := format(
                'rls_row_visible(%L::regclass, ARRAY[%s])',
                owners,
                match_pred
            );
        END IF;
        EXECUTE format(
            'CREATE POLICY rls_select ON %I FOR SELECT USING (%s)',
            rec.table_name,
            match_pred
        );
        EXECUTE format(
            'CREATE POLICY rls_update ON %I FOR UPDATE USING (%s) WITH CHECK (%s)',
            rec.table_name,
            match_pred,
            match_pred
        );
        EXECUTE format(
            'CREATE POLICY rls_delete ON %I FOR DELETE USING (%s)',
            rec.table_name,
            match_pred
        );
        EXECUTE format(
            'CREATE POLICY rls_insert ON %I FOR INSERT
             WITH CHECK (
                rls_is_bypass()
                OR (
                    rls_current_company_id() IS NOT NULL
                    AND rls_is_company_member(rls_current_company_id())
                )
             )',
            rec.table_name
        );
        EXECUTE format(
            'CREATE POLICY rls_select ON %I FOR SELECT
             USING (
                rls_is_bypass()
                OR (
                    company_id = rls_current_company_id()
                    AND rls_is_company_member(company_id)
                )
             )',
            owners
        );
        EXECUTE format(
            'CREATE POLICY rls_delete ON %I FOR DELETE
             USING (
                rls_is_bypass()
                OR (
                    company_id = rls_current_company_id()
                    AND rls_is_company_member(company_id)
                )
             )',
            owners
        );
        EXECUTE format(
            'CREATE POLICY rls_insert ON %I FOR INSERT
             WITH CHECK (
                rls_is_bypass()
                OR (
                    company_id = rls_current_company_id()
                    AND rls_is_company_member(company_id)
                )
             )',
            owners
        );
        FOR con_name IN
            SELECT con.conname
            FROM pg_constraint con
            WHERE con.conrelid = to_regclass(format('public.%I', owners))
              AND con.contype = 'f'
              AND NOT con.condeferrable
        LOOP
            EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', owners, con_name);
        END LOOP;
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint con
            WHERE con.conrelid = to_regclass(format('public.%I', owners))
              AND con.contype = 'f'
              AND (
                  SELECT a.attname FROM pg_attribute a
                  WHERE a.attrelid = con.conrelid AND a.attnum = ANY (con.conkey)
                  ORDER BY a.attnum LIMIT 1
              ) IS DISTINCT FROM 'company_id'
        ) THEN
            IF pk_count = 1 AND pk_names[1] = 'id' THEN
                EXECUTE format(
                    'ALTER TABLE %I ADD CONSTRAINT %I FOREIGN KEY (record_id) REFERENCES %I (id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED',
                    owners,
                    'rls_fk_' || substr(md5(rec.table_name), 1, 16),
                    rec.table_name
                );
            ELSE
                EXECUTE format(
                    'ALTER TABLE %I ADD CONSTRAINT %I FOREIGN KEY (%s) REFERENCES %I (%s) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED',
                    owners,
                    'rls_fk_' || substr(md5(rec.table_name), 1, 16),
                    fk_list,
                    rec.table_name,
                    fk_list
                );
            END IF;
        END IF;
        EXECUTE format('DROP TRIGGER IF EXISTS rls_own_row ON %I', rec.table_name);
        EXECUTE format(
            'CREATE TRIGGER rls_own_row BEFORE INSERT ON %I
             FOR EACH ROW EXECUTE FUNCTION rls_own_new_row(%L, %L, %L)',
            rec.table_name,
            owners,
            owner_cols,
            source_cols
        );
    END LOOP;

    PERFORM rls_install_filesystem_preferences();

    -- App Manager is gone; drop any leftover install table and its owners sidecar.
    DELETE FROM rls_owner_tables WHERE base_table = 'installed_apps';
    EXECUTE format(
        'DROP TABLE IF EXISTS %I CASCADE',
        rls_owners_name('installed_apps')
    );
    EXECUTE 'DROP TABLE IF EXISTS installed_apps CASCADE';

    -- companies: members + superusers see rows; owners/superusers mutate;
    -- any signed-in user inserts. Bypass covers admin/migration sessions.
    IF to_regclass('public.companies') IS NOT NULL THEN
        EXECUTE 'ALTER TABLE companies ENABLE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_select ON companies';
        EXECUTE 'DROP POLICY IF EXISTS rls_update ON companies';
        EXECUTE 'DROP POLICY IF EXISTS rls_delete ON companies';
        EXECUTE 'DROP POLICY IF EXISTS rls_insert ON companies';
        EXECUTE $pol$
            CREATE POLICY rls_select ON companies
            FOR SELECT
            USING (
                rls_is_bypass()
                OR rls_is_superuser()
                OR rls_is_company_member(id)
                -- INSERT … RETURNING runs before the owner membership exists.
                OR (
                    rls_current_user_id() IS NOT NULL
                    AND NOT rls_company_has_members(id)
                )
            )
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_update ON companies
            FOR UPDATE
            USING (rls_is_bypass() OR rls_is_superuser() OR rls_is_company_owner(id))
            WITH CHECK (rls_is_bypass() OR rls_is_superuser() OR rls_is_company_owner(id))
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_delete ON companies
            FOR DELETE
            USING (rls_is_bypass() OR rls_is_superuser() OR rls_is_company_owner(id))
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_insert ON companies
            FOR INSERT
            WITH CHECK (rls_is_bypass() OR rls_current_user_id() IS NOT NULL)
        $pol$;
    END IF;

    IF to_regclass('public.company_memberships') IS NOT NULL THEN
        EXECUTE 'ALTER TABLE company_memberships ENABLE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_select ON company_memberships';
        EXECUTE 'DROP POLICY IF EXISTS rls_update ON company_memberships';
        EXECUTE 'DROP POLICY IF EXISTS rls_delete ON company_memberships';
        EXECUTE 'DROP POLICY IF EXISTS rls_insert ON company_memberships';
        EXECUTE $pol$
            CREATE POLICY rls_select ON company_memberships
            FOR SELECT
            USING (
                rls_is_bypass()
                OR rls_is_superuser()
                OR rls_is_company_member(company_id)
                -- Own row visible for INSERT … RETURNING of the first owner.
                OR user_id = rls_current_user_id()
            )
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_update ON company_memberships
            FOR UPDATE
            USING (rls_is_bypass() OR rls_is_superuser() OR rls_is_company_owner(company_id))
            WITH CHECK (rls_is_bypass() OR rls_is_superuser() OR rls_is_company_owner(company_id))
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_delete ON company_memberships
            FOR DELETE
            USING (rls_is_bypass() OR rls_is_superuser() OR rls_is_company_owner(company_id))
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_insert ON company_memberships
            FOR INSERT
            WITH CHECK (
                rls_is_bypass()
                OR rls_is_superuser()
                OR (
                    rls_current_user_id() IS NOT NULL
                    AND (
                        rls_is_company_owner(company_id)
                        OR (
                            user_id = rls_current_user_id()
                            AND owned_by
                            AND NOT rls_company_has_members(company_id)
                        )
                    )
                )
            )
        $pol$;
    END IF;

    IF to_regclass('public.company_user_prefs') IS NOT NULL THEN
        EXECUTE 'ALTER TABLE company_user_prefs ENABLE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_select ON company_user_prefs';
        EXECUTE 'DROP POLICY IF EXISTS rls_update ON company_user_prefs';
        EXECUTE 'DROP POLICY IF EXISTS rls_delete ON company_user_prefs';
        EXECUTE 'DROP POLICY IF EXISTS rls_insert ON company_user_prefs';
        EXECUTE $pol$
            CREATE POLICY rls_select ON company_user_prefs
            FOR SELECT
            USING (user_id = rls_current_user_id())
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_update ON company_user_prefs
            FOR UPDATE
            USING (user_id = rls_current_user_id())
            WITH CHECK (
                user_id = rls_current_user_id()
                AND (
                    (active_company_id IS NULL AND rls_is_superuser())
                    OR (
                        active_company_id IS NOT NULL
                        AND rls_is_company_member(active_company_id)
                    )
                )
            )
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_delete ON company_user_prefs
            FOR DELETE
            USING (user_id = rls_current_user_id())
        $pol$;
        EXECUTE $pol$
            CREATE POLICY rls_insert ON company_user_prefs
            FOR INSERT
            WITH CHECK (
                user_id = rls_current_user_id()
                AND (
                    (active_company_id IS NULL AND rls_is_superuser())
                    OR (
                        active_company_id IS NOT NULL
                        AND rls_is_company_member(active_company_id)
                    )
                )
            )
        $pol$;
    END IF;

    FOR idx IN
        SELECT ic.oid AS index_oid,
               ic.relname AS index_name,
               cls.relname AS table_name,
               cls.oid AS table_oid,
               (
                   SELECT string_agg(a.attname, ',' ORDER BY cols.ord)
                   FROM unnest(i.indkey) WITH ORDINALITY AS cols(attnum, ord)
                   JOIN pg_attribute a
                     ON a.attrelid = i.indrelid
                    AND a.attnum = cols.attnum
               ) AS col_list
        FROM pg_index i
        JOIN pg_class cls ON cls.oid = i.indrelid
        JOIN pg_namespace ns ON ns.oid = cls.relnamespace
        JOIN pg_class ic ON ic.oid = i.indexrelid
        WHERE ns.nspname = 'public'
          AND i.indisunique
          AND NOT i.indisprimary
          AND cls.relkind = 'r'
          AND cls.relname NOT IN (
              'seaql_migrations',
              'roles',
              'users',
              'rls_owner_tables',
              'companies',
              'company_memberships',
              'company_user_prefs',
              'logs',
              'banks',
              'bank_directory_syncs'
          )
          AND cls.relname NOT LIKE 'rlsown\_%' ESCAPE '\'
          AND NOT EXISTS (SELECT 1 FROM unnest(i.indkey) AS k(attnum) WHERE k.attnum = 0)
          AND to_regclass(format('public.%I', rls_owners_name(cls.relname))) IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM pg_attribute a
              WHERE a.attrelid = to_regclass(format('public.%I', rls_owners_name(cls.relname)))
                AND a.attname = 'record_id'
                AND NOT a.attisdropped
          )
    LOOP
        IF EXISTS (
            SELECT 1
            FROM pg_constraint con
            JOIN unnest(string_to_array(idx.col_list, ',')) AS col(name) ON true
            JOIN pg_attribute a
              ON a.attrelid = con.conrelid
             AND a.attnum = ANY (con.conkey)
             AND a.attname = col.name
            WHERE con.conrelid = idx.table_oid
              AND con.contype = 'f'
        ) OR EXISTS (
            -- Another table's foreign key points at this unique index
            -- (bank_accounts.ifsc_code → banks.ifsc). The constraint has to stay.
            SELECT 1
            FROM pg_constraint fk
            WHERE fk.contype = 'f'
              AND fk.conindid = idx.index_oid
        ) THEN
            CONTINUE;
        END IF;

        con_name := NULL;
        SELECT c.conname
        INTO con_name
        FROM pg_constraint c
        WHERE c.conindid = idx.index_oid
          AND c.contype = 'u';
        IF con_name IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I DROP CONSTRAINT IF EXISTS %I',
                idx.table_name,
                con_name
            );
        ELSE
            EXECUTE format('DROP INDEX IF EXISTS public.%I', idx.index_name);
        END IF;

        col_list := idx.col_list;
        IF col_list IS NULL OR col_list = '' THEN
            CONTINUE;
        END IF;
        EXECUTE format(
            'DROP TRIGGER IF EXISTS %I ON %I',
            'rls_uq_' || idx.index_oid::text,
            idx.table_name
        );
        EXECUTE format(
            'CREATE TRIGGER %I AFTER INSERT OR UPDATE ON %I
             FOR EACH ROW EXECUTE FUNCTION rls_unique_per_owner(%L, %L, %L)',
            'rls_uq_' || idx.index_oid::text,
            idx.table_name,
            rls_owners_name(idx.table_name),
            'record_id',
            col_list
        );
    END LOOP;

    FOR idx IN
        SELECT c.relname AS table_name,
               t.tgname AS trigger_name,
               (regexp_split_to_array(encode(t.tgargs, 'escape'), E'\\\\000'))[3] AS cols
        FROM pg_trigger t
        JOIN pg_proc p ON p.oid = t.tgfoid
        JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND NOT t.tgisinternal
          AND p.proname = 'rls_unique_per_owner'
    LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', idx.trigger_name, idx.table_name);
        IF idx.cols IS NOT NULL AND idx.cols <> '' THEN
            EXECUTE format(
                'CREATE TRIGGER %I AFTER INSERT OR UPDATE ON %I
                 FOR EACH ROW EXECUTE FUNCTION rls_unique_per_owner(%L, %L, %L)',
                idx.trigger_name,
                idx.table_name,
                rls_owners_name(idx.table_name),
                'record_id',
                idx.cols
            );
        END IF;
    END LOOP;

    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO lariv_website_app';
    EXECUTE 'GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO lariv_website_app';
    EXECUTE 'ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO lariv_website_app';
    EXECUTE 'ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO lariv_website_app';
END
$fn$;

DROP FUNCTION IF EXISTS rls_grant(regclass, bigint, bigint);
DROP FUNCTION IF EXISTS rls_revoke(regclass, bigint, bigint);

REVOKE ALL ON FUNCTION rls_backfill(bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION rls_admin_company_id(bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION rls_find_user_by_email(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION rls_find_user_by_email(text) TO lariv_website_app;
REVOKE ALL ON FUNCTION rls_signup_user(text, text, text, bytea, bytea, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION rls_signup_user(text, text, text, bytea, bytea, text) TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_is_company_member(bigint) TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_is_company_owner(bigint) TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_company_has_members(bigint) TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_current_company_id() TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_resolve_active_company() TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_is_superuser() TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_is_bypass() TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_row_visible_optional_company(regclass, bigint[]) TO lariv_website_app;
REVOKE ALL ON FUNCTION rls_filesystem_pref_visible(regclass, bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION rls_filesystem_pref_visible(regclass, bigint) TO lariv_website_app;
GRANT EXECUTE ON FUNCTION rls_own_new_row_optional_company() TO lariv_website_app;
REVOKE ALL ON FUNCTION rls_public_website_vnode(bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION rls_public_website_vnode(bigint) TO lariv_website_app;
REVOKE ALL ON FUNCTION rls_install_public_website_table(text) FROM PUBLIC;

DO $$ BEGIN PERFORM rls_install_policies(); END $$;
