-- Per-user Gemini keys stay on their own row. Site-wide fallbacks are in
-- global_preferences. This script drops the older copy triggers.
-- Applied after row-level security, which creates rls_owners_name and the owners table.

CREATE OR REPLACE FUNCTION rls_canonical_gemini_pref(OUT record_id bigint, OUT api_key text)
RETURNS record
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public
AS $fn$
DECLARE
    owners text;
BEGIN
    record_id := NULL;
    api_key := NULL;
    IF to_regclass('public.llm_assistant_preferences') IS NULL THEN
        RETURN;
    END IF;
    owners := rls_owners_name('llm_assistant_preferences');
    IF to_regclass(format('public.%I', owners)) IS NULL THEN
        RETURN;
    END IF;
    EXECUTE format(
        'SELECT o.record_id, p.api_key
         FROM %I o
         JOIN llm_assistant_preferences p ON p.id = o.record_id
         WHERE o.company_id IS NULL
         ORDER BY o.record_id
         LIMIT 1',
        owners
    )
    INTO record_id, api_key;
END
$fn$;

-- Kept so older installs can drop the trigger. It no longer rewrites keys:
-- each row keeps the key its user saved, and global_preferences fills gaps.
CREATE OR REPLACE FUNCTION rls_align_gemini_key()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $fn$
BEGIN
    IF TG_WHEN = 'AFTER' THEN
        RETURN NULL;
    END IF;
    RETURN NEW;
END
$fn$;

CREATE OR REPLACE FUNCTION rls_install_gemini_key()
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $fn$
BEGIN
    IF to_regclass('public.llm_assistant_preferences') IS NULL THEN
        RETURN;
    END IF;
    EXECUTE 'DROP TRIGGER IF EXISTS rls_gemini_key_write ON llm_assistant_preferences';
    EXECUTE 'DROP TRIGGER IF EXISTS rls_gemini_key_fanout ON llm_assistant_preferences';
END
$fn$;

REVOKE ALL ON FUNCTION rls_canonical_gemini_pref() FROM PUBLIC;
REVOKE ALL ON FUNCTION rls_install_gemini_key() FROM PUBLIC;
REVOKE ALL ON FUNCTION rls_align_gemini_key() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION rls_align_gemini_key() TO lariv_website_app;

DO $$ BEGIN PERFORM rls_install_gemini_key(); END $$;
