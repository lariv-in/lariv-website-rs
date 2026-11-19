--
-- PostgreSQL database dump
--

\restrict 1VYF7o7MsMuI8yeZyx3Sw94RnhRgxKSFIwFtwQpBR4hrur5TEFlYJEY1fBlGtt1

-- Dumped from database version 17.10
-- Dumped by pg_dump version 17.12 (Debian 17.12-0+deb13u1)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: ltree; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS ltree WITH SCHEMA public;


--
-- Name: EXTENSION ltree; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION ltree IS 'data type for hierarchical tree-like structures';


--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: path_to_ltree(text); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.path_to_ltree(p text) RETURNS public.ltree
    LANGUAGE plpgsql IMMUTABLE
    SET search_path TO 'public'
    AS $$
DECLARE
    cleaned text;
BEGIN
    cleaned := trim(both '/' from p);
    IF cleaned = '' THEN
        RETURN 'root'::public.ltree;
    END IF;
    cleaned := replace(replace(cleaned, '/', '.'), '-', '_');
    cleaned := regexp_replace(cleaned, '[^a-zA-Z0-9_\.]', '', 'g');
    cleaned := regexp_replace(cleaned, '\.\.', '.', 'g');
    cleaned := trim(both '.' from cleaned);
    IF cleaned = '' THEN
        RETURN 'root'::public.ltree;
    END IF;
    RETURN cleaned::public.ltree;
EXCEPTION WHEN OTHERS THEN
    RETURN 'root'::public.ltree;
END;
$$;


ALTER FUNCTION public.path_to_ltree(p text) OWNER TO postgres;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: blog_tags; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.blog_tags (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    name text NOT NULL
);


ALTER TABLE public.blog_tags OWNER TO postgres;

--
-- Name: blog_tags_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.blog_tags_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.blog_tags_id_seq OWNER TO postgres;

--
-- Name: blog_tags_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.blog_tags_id_seq OWNED BY public.blog_tags.id;


--
-- Name: blogs; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.blogs (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    title text NOT NULL,
    slug character varying(255) NOT NULL,
    description text,
    created_by_id bigint NOT NULL,
    content text
);


ALTER TABLE public.blogs OWNER TO postgres;

--
-- Name: blogs_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.blogs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.blogs_id_seq OWNER TO postgres;

--
-- Name: blogs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.blogs_id_seq OWNED BY public.blogs.id;


--
-- Name: db_routes; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.db_routes (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    path text NOT NULL,
    page_id bigint NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    theme character varying(128) DEFAULT ''::character varying NOT NULL,
    grapes_project text,
    ltree_path public.ltree GENERATED ALWAYS AS (public.path_to_ltree(path)) STORED
);


ALTER TABLE public.db_routes OWNER TO postgres;

--
-- Name: db_routes_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.db_routes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.db_routes_id_seq OWNER TO postgres;

--
-- Name: db_routes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.db_routes_id_seq OWNED BY public.db_routes.id;


--
-- Name: filesystem_git_syncs; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.filesystem_git_syncs (
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    git_url text NOT NULL,
    git_identifier text NOT NULL,
    subpath text NOT NULL,
    directory_vnode_id bigint NOT NULL,
    last_synced_commit_hash text
);


ALTER TABLE public.filesystem_git_syncs OWNER TO postgres;

--
-- Name: filesystem_nodes; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.filesystem_nodes (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    name text NOT NULL,
    is_directory boolean NOT NULL,
    file_path text,
    parent_id bigint,
    owner_id bigint,
    permissions integer NOT NULL,
    role text
);


ALTER TABLE public.filesystem_nodes OWNER TO postgres;

--
-- Name: filesystem_nodes_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.filesystem_nodes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.filesystem_nodes_id_seq OWNER TO postgres;

--
-- Name: filesystem_nodes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.filesystem_nodes_id_seq OWNED BY public.filesystem_nodes.id;


--
-- Name: filesystem_preferences; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.filesystem_preferences (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    owner_id bigint,
    permissions integer NOT NULL,
    role text
);


ALTER TABLE public.filesystem_preferences OWNER TO postgres;

--
-- Name: filesystem_preferences_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

ALTER TABLE public.filesystem_preferences ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.filesystem_preferences_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: mdbook_preferences; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.mdbook_preferences (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    source_vnode_id bigint NOT NULL,
    output_vnode_id bigint NOT NULL,
    url_prefix text DEFAULT '/book'::text NOT NULL,
    build_on_serve boolean DEFAULT true NOT NULL
);


ALTER TABLE public.mdbook_preferences OWNER TO postgres;

--
-- Name: p_blog_tags; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.p_blog_tags (
    blog_id bigint NOT NULL,
    blog_tag_id bigint NOT NULL
);


ALTER TABLE public.p_blog_tags OWNER TO postgres;

--
-- Name: p_website_route_references; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.p_website_route_references (
    db_route_id bigint NOT NULL,
    v_node_id bigint NOT NULL
);


ALTER TABLE public.p_website_route_references OWNER TO postgres;

--
-- Name: seaql_migrations; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.seaql_migrations (
    version character varying NOT NULL,
    applied_at bigint NOT NULL
);


ALTER TABLE public.seaql_migrations OWNER TO postgres;

--
-- Name: users; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.users (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    name text NOT NULL,
    email text,
    phone text DEFAULT ''::text NOT NULL,
    password bytea,
    password_salt bytea,
    timezone text DEFAULT 'Asia/Kolkata'::text,
    last_login_time timestamp with time zone,
    role text NOT NULL
);


ALTER TABLE public.users OWNER TO postgres;

--
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.users_id_seq OWNER TO postgres;

--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- Name: website_preferences; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.website_preferences (
    id bigint NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    custom_theme_css_vnode_id bigint,
    custom_theme_js_vnode_id bigint
);


ALTER TABLE public.website_preferences OWNER TO postgres;

--
-- Name: blog_tags id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.blog_tags ALTER COLUMN id SET DEFAULT nextval('public.blog_tags_id_seq'::regclass);


--
-- Name: blogs id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.blogs ALTER COLUMN id SET DEFAULT nextval('public.blogs_id_seq'::regclass);


--
-- Name: db_routes id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.db_routes ALTER COLUMN id SET DEFAULT nextval('public.db_routes_id_seq'::regclass);


--
-- Name: filesystem_nodes id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_nodes ALTER COLUMN id SET DEFAULT nextval('public.filesystem_nodes_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- Data for Name: blog_tags; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.blog_tags (id, created_at, updated_at, name) FROM stdin;
\.


--
-- Data for Name: blogs; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.blogs (id, created_at, updated_at, title, slug, description, created_by_id, content) FROM stdin;
1	2026-09-01 17:46:51.783406+00	2026-09-06 08:30:48.791909+00	Why Framework Speed Matters in the Age of AI	why-framework-speed-matters-in-the-age-of-ai	How we benchmarked Lariv against Odoo and Frappe on identical hosts at 1, 50, and 500 concurrent users (parsing, realistic CRUD, and AI queue workloads) and why runtime speed now matters.	1	With the introduction of AI, software development speed has skyrocketed, allowing teams to build features faster than ever. Today, the real challenge is the speed and runtime efficiency of the framework itself. As long as a framework is open source and not closed source, it can be adapted to fit any business use case—and Lariv delivers both top-tier speed and high efficiency out of the box.\n\n# Performance Methodology\n\nTo evaluate real-world performance, we tested all frameworks across three distinct benchmark flows using identical hosting environments and 500 concurrent worker stress tests. Each benchmark flow tests a specific layer of framework performance to provide a complete picture of speed and efficiency.\n\n## Parsing Benchmark\n\nThis benchmark involves testing the request parsing performance of the framework. It is measuring pure framework speed by isolating HTTP request parsing and JSON data serialization without any database overhead.\n\n[Jump to Parsing Results ↓](#parsing-benchmark-results)\n\n## Realistic Load Benchmark\n\nThis benchmark represents what a typical, real-world application request looks like. It measures the complete end-to-end framework performance, from initial request parsing down to SQL query building, database communication, and response rendering.\n\n[Jump to Realistic Load Results ↓](#realistic-load-benchmark-results)\n\n## AI Workload Benchmark\n\nWe included the AI workload benchmark because modern applications frequently run heavy background tasks and AI pipelines. Its significance is measuring the framework's internal overhead and queue efficiency when running long-running processes.\n\n[Jump to AI Workload Results ↓](#ai-workload-benchmark-results)\n\n# Benchmark Results\n\nBelow are the detailed benchmark results and performance charts measured across 1, 50, and 500 simultaneous users.\n\n<a id="parsing-benchmark-results"></a>\n\n## Parsing Benchmark Results\n\nThe charts below display data parsing and serialization throughput and average response latency across different user levels.\n\n### Parsing Test: 1 User\n\nThis measures data parsing and serialization speed for a single user processing requests in RAM. Lariv processes over 27,000 requests per second in about 0.03 milliseconds, proving its compiled engine handles request parsing far faster than other servers.\n\n<div class="perf-pair" data-article-benchmark="counter" data-workers="1"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="counter-rps-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="counter-latency-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n### Parsing Test: 50 Users\n\nThis measures data parsing processing speed across 50 simultaneous users. Lariv exceeds 145,000 requests per second at about 0.3 milliseconds, smoothly splitting work across processor cores, while other systems stay under 1,000 requests per second and hit processing bottlenecks due to single-thread locks.\n\n<div class="perf-pair" data-article-benchmark="counter" data-workers="50"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="counter-rps-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="counter-latency-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n### Parsing Test: 500 Users\n\nThis tests data parsing speed under heavy load with 500 simultaneous connections. Under peak traffic, Lariv sustains over 176,000 requests per second with delays under 3 milliseconds using fast modern networking, while traditional servers drop below 1,000 requests per second and slow down due to queuing.\n\n<div class="perf-pair" data-article-benchmark="counter" data-workers="500"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="counter-rps-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="counter-latency-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n<a id="realistic-load-benchmark-results"></a>\n\n## Realistic Load Benchmark Results\n\nThe charts below display end-to-end database request throughput and response times under typical application workloads.\n\n### Realistic Load Test: 1 User\n\nThis test measures performance for a single user making database requests one after another. Even with just one user, Lariv runs 4 to 8 times faster than other systems, responding in about 0.74 milliseconds at over 1,340 requests per second.\n\n<div class="perf-pair" data-article-benchmark="crud" data-workers="1"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="crud-rps-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="crud-latency-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n### Realistic Load Test: 50 Users\n\nThis test simulates a medium-sized team placing orders and viewing reports at the same time. At 50 users, other systems slow down to 110–130 milliseconds per request, while Lariv stays fast at about 18,900 requests per second with response times under 3 milliseconds.\n\n<div class="perf-pair" data-article-benchmark="crud" data-workers="50"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="crud-rps-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="crud-latency-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n### Realistic Load Test: 500 Users\n\nThis test simulates heavy traffic during shift changes and peak business hours. At 500 users, other systems overload with delays around 1 second and large numbers of failed requests, while Lariv processes over 19,600 requests per second cleanly in about 25 milliseconds.\n\n<div class="perf-pair" data-article-benchmark="crud" data-workers="500"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="crud-rps-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="crud-latency-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n<a id="ai-workload-benchmark-results"></a>\n\n## AI Workload Benchmark Results\n\nThe charts below display task queue processing capacity and execution delays for background tasks and AI pipelines.\n\n### AI Workload Test: 1 User\n\nThis measures single-user speed for sending tasks to a background queue and getting results back. Lariv processes over 13,000 tasks per second with about 0.08 milliseconds of delay, adding virtually no lag to AI workflows.\n\n<div class="perf-pair" data-article-benchmark="task" data-workers="1"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="task-rps-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="task-latency-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n### AI Workload Test: 50 Users\n\nThis measures background queue processing across 50 simultaneous users. Lariv handles over 66,000 tasks per second with delays under 1 millisecond, keeping background jobs running smoothly, while other systems fall to a handful of successful tasks per second.\n\n<div class="perf-pair" data-article-benchmark="task" data-workers="50"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="task-rps-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="task-latency-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n### AI Workload Test: 500 Users\n\nThis tests background task processing under heavy load with 500 simultaneous users. Lariv processes over 76,000 tasks per second with delays under 7 milliseconds, proving it can handle demanding AI agent pipelines. Odoo and Frappe drop to zero successful requests under the same load.\n\n<div class="perf-pair" data-article-benchmark="task" data-workers="500"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="task-rps-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="task-latency-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>\n\n<p class="article-source"><a href="https://raw.githubusercontent.com/lariv-in/benchmarks/refs/heads/main/benchmark_metrics.json" target="_blank" rel="noopener">Data sourced from GitHub: lariv-in/benchmarks ↗</a></p>\n\n## Why User Scale Matters\n\nTraditional ERP systems hit a hard performance limit around 400–500 requests per second, causing user waiting times to jump from tens of milliseconds to around 1 second, with large numbers of failed requests. Lariv avoids these delays by separating network connections from internal processing, sustaining over 19,600 requests per second at about 25 milliseconds during peak traffic.\n\n## Choosing the Right Framework\n\nChoosing an application framework is an important business decision, and core speed determines how reliably it runs as your company grows. While other systems like Odoo and Frappe slow down under heavy traffic, Lariv handles high loads with fast speeds, low delays, and zero dropped requests, keeping server costs low and work moving quickly.\n\n## Potential Cost Benefits\n\nPerformance directly impacts server bills and infrastructure costs. Choosing Lariv over traditional frameworks offers substantial financial advantages:\n\n- **Up to 80–90% Reduction in Infrastructure Spending:** Because Lariv handles over 19,600 requests per second compared to ~400–500 RPS on traditional frameworks like Odoo or Frappe, a single server running Lariv can process workloads that would normally require a cluster of multiple application servers behind a load balancer.\n- **Lower Memory & CPU Footprint:** Traditional frameworks rely on multi-process workers that consume gigabytes of RAM under concurrent traffic. Lariv's compiled engine handles thousands of simultaneous connections with minimal memory and CPU consumption.\n- **Reduced Maintenance and Operations:** Fewer active servers mean simplified deployment pipelines, lower cloud monitoring overhead, fewer points of system failure, and reduced DevOps operational costs.\n- **Longer Hardware Lifetime:** As your organization grows, Lariv scales effortlessly on existing hosting tiers, deferring the need for expensive infrastructure upgrades and dedicated cloud instances.\n\n### Try the Benchmark Code Yourself\n\nAll benchmark scripts, test data, and setup steps are open source so anyone can test and verify our results. You can view the code and run the tests on your own servers by visiting the [Lariv Benchmarks GitHub Repository](https://github.com/lariv-in/benchmarks).\n
\.


--
-- Data for Name: db_routes; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.db_routes (id, created_at, updated_at, path, page_id, is_active, theme, grapes_project) FROM stdin;
1	2026-09-01 17:43:24.043296+00	2026-09-17 16:32:40.007752+00	/static/logo.svg	6	t		\N
2	2026-09-01 17:43:24.123272+00	2026-09-17 16:32:40.043316+00	/static/main-artwork.svg	7	t		\N
3	2026-09-01 17:43:24.19232+00	2026-09-17 16:32:40.080303+00	/static/benchmark_metrics.json	8	t		\N
8	2026-09-01 17:43:24.781181+00	2026-11-09 10:38:56.008564+00	/privacy-policy	17	t	p_website.custom	\N
7	2026-09-01 17:43:24.706108+00	2026-11-09 10:39:05.971135+00	/blogs/*	16	t	p_website.custom	\N
6	2026-09-01 17:43:24.629944+00	2026-11-09 10:39:16.292503+00	/blogs	15	t	p_website.custom	\N
5	2026-09-01 17:43:24.554378+00	2026-11-09 10:39:25.86178+00	/pricing	14	t	p_website.custom	\N
4	2026-09-01 17:43:24.399197+00	2026-11-09 10:39:33.926744+00	/	12	t	p_website.custom	\N
\.


--
-- Data for Name: filesystem_git_syncs; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.filesystem_git_syncs (created_at, updated_at, git_url, git_identifier, subpath, directory_vnode_id, last_synced_commit_hash) FROM stdin;
2026-11-09 10:38:17.399217+00	2026-11-09 10:40:00.267277+00	https://github.com/lariv-in/lariv-website-html.git	master		1	2b1375bf2efe551bc1a6f5a1cb0e702755b32312
\.


--
-- Data for Name: filesystem_nodes; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.filesystem_nodes (id, created_at, updated_at, name, is_directory, file_path, parent_id, owner_id, permissions, role) FROM stdin;
1	2026-09-01 17:43:23.786685+00	2026-09-01 17:43:23.786685+00	website	t	\N	\N	\N	511	\N
2	2026-09-01 17:43:23.794723+00	2026-09-01 17:43:23.794723+00	themes	t	\N	1	\N	511	\N
5	2026-09-01 17:43:23.95393+00	2026-09-01 17:43:23.95393+00	static	t	\N	1	\N	511	\N
11	2026-09-01 17:43:24.32541+00	2026-09-01 17:43:24.32541+00	pages	t	\N	1	\N	511	\N
22	2026-11-09 10:37:50.265938+00	2026-11-09 10:37:50.265938+00	mdbook	t	\N	\N	\N	367	\N
23	2026-11-09 10:37:50.273613+00	2026-11-09 10:37:50.273613+00	mdbook-site	t	\N	\N	\N	367	\N
15	2026-09-01 17:43:24.625878+00	2026-11-09 10:39:58.806006+00	blogs.html	f	lariv/89fb172595d6403d85010d79ca4df2b3.html	11	\N	511	\N
16	2026-09-01 17:43:24.702261+00	2026-11-09 10:39:58.924277+00	blogs_slug.html	f	lariv/f868eea4d19a4870876ee5a6f0c84eb7.html	11	\N	511	\N
12	2026-09-01 17:43:24.394585+00	2026-11-09 10:39:59.034576+00	index.html	f	lariv/4af6c88b8e324c2988ae517e4adb3391.html	11	\N	511	\N
14	2026-09-01 17:43:24.550347+00	2026-11-09 10:39:59.153209+00	pricing.html	f	lariv/182b2cfcbf5947368a7e5bf5707bb2a8.html	11	\N	511	\N
17	2026-09-01 17:43:24.777813+00	2026-11-09 10:39:59.260403+00	privacy-policy.html	f	lariv/cd4b0da8363c4cd4aa92df1d1c9fb128.html	11	\N	511	\N
8	2026-09-01 17:43:24.188359+00	2026-11-09 10:39:59.372444+00	benchmark_metrics.json	f	lariv/8982fef272d64fd2a8b99e7ef2e69ef6.json	5	\N	511	\N
6	2026-09-01 17:43:24.037053+00	2026-11-09 10:39:59.484558+00	logo.svg	f	lariv/07ebca84c0954ea6a8851a1a2d2bd25f.svg	5	\N	511	\N
7	2026-09-01 17:43:24.118669+00	2026-11-09 10:39:59.592187+00	main-artwork.svg	f	lariv/c9fbcaf06ff24d63845652f83adf4e30.svg	5	\N	511	\N
3	2026-09-01 17:43:23.878172+00	2026-11-09 10:39:59.697916+00	lariv.css	f	lariv/333ad5b60da4493fa64d9ced43e88ff5.css	2	\N	511	\N
4	2026-09-01 17:43:23.938901+00	2026-11-09 10:39:59.816011+00	lariv.js	f	lariv/e47667a4cb7342d6a76d04073d07b430.js	2	\N	511	\N
10	2026-09-01 17:43:24.318299+00	2026-11-09 10:40:00.013326+00	footer.html	f	lariv/b456b08411bf40e5a68cfd204ec6a195.html	1	\N	511	\N
9	2026-09-01 17:43:24.256581+00	2026-11-09 10:40:00.10302+00	header.html	f	lariv/e607848ddfac4a5287fd156cb56bf8a2.html	1	\N	511	\N
\.


--
-- Data for Name: filesystem_preferences; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.filesystem_preferences (id, created_at, updated_at, owner_id, permissions, role) FROM stdin;
1	2026-11-09 10:37:49.965806+00	2026-11-09 10:37:49.965806+00	\N	511	\N
\.


--
-- Data for Name: mdbook_preferences; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.mdbook_preferences (id, created_at, updated_at, source_vnode_id, output_vnode_id, url_prefix, build_on_serve) FROM stdin;
1	2026-11-09 10:37:50.261117+00	2026-11-09 10:37:50.261117+00	22	23	/book	t
\.


--
-- Data for Name: p_blog_tags; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.p_blog_tags (blog_id, blog_tag_id) FROM stdin;
\.


--
-- Data for Name: p_website_route_references; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.p_website_route_references (db_route_id, v_node_id) FROM stdin;
8	9
8	10
8	3
8	4
7	9
7	10
7	3
7	4
6	9
6	10
6	3
6	4
5	9
5	10
5	3
5	4
4	9
4	10
4	3
4	4
\.


--
-- Data for Name: seaql_migrations; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.seaql_migrations (version, applied_at) FROM stdin;
m20240729_000001_create_users_roles	1788284601
m20260808_000001_users_drop_deleted_at	1788284601
m20260817_000001_users_phone_default	1788284601
m20260730_000002_create_filesystem_nodes	1788284601
m20260808_000001_filesystem_drop_deleted_at	1788284601
m20260730_000001_create_blog_tables	1788284601
m20260808_000001_blog_drop_deleted_at	1788284601
m20260731_000001_create_db_routes	1788284601
m20260808_000001_website_drop_deleted_at	1788284601
m20260829_000001_create_website_preferences	1788284601
m20260829_000002_website_preferences_js	1788284601
m20260910_000001_users_email_pg_trgm	1789662577
m20260930_000001_roles_title_description	1794220669
m20261012_000001_users_last_login_time	1794220669
m20260928_000001_filesystem_node_permissions	1794220669
m20261003_000001_filesystem_root_permissions	1794220669
m20261003_000002_role_names	1794220670
m20261003_000003_users_drop_is_superuser	1794220670
m20261108_000001_create_mdbook_preferences	1794220670
m20261108_000002_rename_mdbook_preferences	1794220670
m20261108_000003_mdbook_preferences_vnode_fkeys	1794220670
m20261108_000001_create_filesystem_git_syncs	1794220670
m20261108_000002_git_identifier_and_last_synced	1794220670
m20261108_000003_directory_primary_key	1794220670
\.


--
-- Data for Name: users; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.users (id, created_at, updated_at, name, email, phone, password, password_salt, timezone, last_login_time, role) FROM stdin;
1	2026-09-01 17:43:23.776362+00	2026-09-01 17:47:31.894257+00	Lariv	superadmin@lariv.in	admin-superadmin@lariv.in	\\x2e072572bb42506417d069386736868c3fdd50eccb5216083891d08b953bc7c7	\\xf15818d9aedfa51f6dd4c8f830bfa49a7f36a9f62d98f0a68843f3b4b91643e93276a313ab87e5e3d94b111c71861b88ff50c6350a5d5e9ea9b253e309ebca192b2b331b1396801c1bf983adc8a5ad2c1bf6a0db92ea4685af256e4810644c21a901c6398c6e1c6751260f57f482bf54b5f7069f1339cfae00f49596507fc9365200a57bb9a5408dcb423670f40c99a0e3045657222ad9793393f91fd6ce0fb8e4e4c850ff82dafab37c98b77120d1f03f1e2c0e3ff525104cc202ec19b0eb8cef25ffa311b9e0c1c0617c2fab8e29ffee6917d80ca42987f0b07a44a9a0910fe11d03d92cb91cfaf4a0a1b90f70f22c559c5a2e70e12d84f976704391a04c0a	Asia/Kolkata	\N	superuser
\.


--
-- Data for Name: website_preferences; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.website_preferences (id, created_at, updated_at, custom_theme_css_vnode_id, custom_theme_js_vnode_id) FROM stdin;
1	2026-09-01 17:43:23.943927+00	2026-09-17 16:32:39.952904+00	3	4
\.


--
-- Name: blog_tags_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.blog_tags_id_seq', 1, false);


--
-- Name: blogs_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.blogs_id_seq', 1, true);


--
-- Name: db_routes_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.db_routes_id_seq', 8, true);


--
-- Name: filesystem_nodes_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.filesystem_nodes_id_seq', 23, true);


--
-- Name: filesystem_preferences_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.filesystem_preferences_id_seq', 1, false);


--
-- Name: users_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.users_id_seq', 2, true);


--
-- Name: blog_tags blog_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.blog_tags
    ADD CONSTRAINT blog_tags_pkey PRIMARY KEY (id);


--
-- Name: blogs blogs_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.blogs
    ADD CONSTRAINT blogs_pkey PRIMARY KEY (id);


--
-- Name: db_routes db_routes_path_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.db_routes
    ADD CONSTRAINT db_routes_path_key UNIQUE (path);


--
-- Name: db_routes db_routes_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.db_routes
    ADD CONSTRAINT db_routes_pkey PRIMARY KEY (id);


--
-- Name: filesystem_git_syncs filesystem_git_syncs_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_git_syncs
    ADD CONSTRAINT filesystem_git_syncs_pkey PRIMARY KEY (directory_vnode_id);


--
-- Name: filesystem_nodes filesystem_nodes_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_nodes
    ADD CONSTRAINT filesystem_nodes_pkey PRIMARY KEY (id);


--
-- Name: filesystem_preferences filesystem_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_preferences
    ADD CONSTRAINT filesystem_preferences_pkey PRIMARY KEY (id);


--
-- Name: mdbook_preferences mdbook_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.mdbook_preferences
    ADD CONSTRAINT mdbook_preferences_pkey PRIMARY KEY (id);


--
-- Name: p_blog_tags p_blog_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.p_blog_tags
    ADD CONSTRAINT p_blog_tags_pkey PRIMARY KEY (blog_id, blog_tag_id);


--
-- Name: p_website_route_references p_website_route_references_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.p_website_route_references
    ADD CONSTRAINT p_website_route_references_pkey PRIMARY KEY (db_route_id, v_node_id);


--
-- Name: seaql_migrations seaql_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.seaql_migrations
    ADD CONSTRAINT seaql_migrations_pkey PRIMARY KEY (version);


--
-- Name: users users_email_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- Name: users users_phone_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_phone_key UNIQUE (phone);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: website_preferences website_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.website_preferences
    ADD CONSTRAINT website_preferences_pkey PRIMARY KEY (id);


--
-- Name: idx_blog_tags_name; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_blog_tags_name ON public.blog_tags USING btree (name);


--
-- Name: idx_blogs_created_by_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_blogs_created_by_id ON public.blogs USING btree (created_by_id);


--
-- Name: idx_blogs_slug; Type: INDEX; Schema: public; Owner: postgres
--

CREATE UNIQUE INDEX idx_blogs_slug ON public.blogs USING btree (slug);


--
-- Name: idx_db_routes_ltree_path; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_db_routes_ltree_path ON public.db_routes USING gist (ltree_path);


--
-- Name: idx_filesystem_nodes_owner_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_filesystem_nodes_owner_id ON public.filesystem_nodes USING btree (owner_id);


--
-- Name: idx_filesystem_nodes_parent_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_filesystem_nodes_parent_id ON public.filesystem_nodes USING btree (parent_id);


--
-- Name: idx_filesystem_nodes_parent_name_dir; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_filesystem_nodes_parent_name_dir ON public.filesystem_nodes USING btree (parent_id, name, is_directory);


--
-- Name: idx_filesystem_preferences_owner_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_filesystem_preferences_owner_id ON public.filesystem_preferences USING btree (owner_id);


--
-- Name: idx_mdbook_preferences_output_vnode_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_mdbook_preferences_output_vnode_id ON public.mdbook_preferences USING btree (output_vnode_id);


--
-- Name: idx_mdbook_preferences_source_vnode_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_mdbook_preferences_source_vnode_id ON public.mdbook_preferences USING btree (source_vnode_id);


--
-- Name: users_email_trgm_idx; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX users_email_trgm_idx ON public.users USING gin (lower(email) public.gin_trgm_ops);


--
-- Name: blogs fk_blogs_created_by_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.blogs
    ADD CONSTRAINT fk_blogs_created_by_id FOREIGN KEY (created_by_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: db_routes fk_db_routes_page_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.db_routes
    ADD CONSTRAINT fk_db_routes_page_id FOREIGN KEY (page_id) REFERENCES public.filesystem_nodes(id) ON DELETE RESTRICT;


--
-- Name: filesystem_git_syncs fk_filesystem_git_syncs_directory_vnode_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_git_syncs
    ADD CONSTRAINT fk_filesystem_git_syncs_directory_vnode_id FOREIGN KEY (directory_vnode_id) REFERENCES public.filesystem_nodes(id) ON UPDATE CASCADE ON DELETE RESTRICT;


--
-- Name: filesystem_nodes fk_filesystem_nodes_owner_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_nodes
    ADD CONSTRAINT fk_filesystem_nodes_owner_id FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: filesystem_nodes fk_filesystem_nodes_parent_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_nodes
    ADD CONSTRAINT fk_filesystem_nodes_parent_id FOREIGN KEY (parent_id) REFERENCES public.filesystem_nodes(id) ON DELETE CASCADE;


--
-- Name: filesystem_preferences fk_filesystem_preferences_owner_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.filesystem_preferences
    ADD CONSTRAINT fk_filesystem_preferences_owner_id FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: mdbook_preferences fk_mdbook_preferences_output_vnode_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.mdbook_preferences
    ADD CONSTRAINT fk_mdbook_preferences_output_vnode_id FOREIGN KEY (output_vnode_id) REFERENCES public.filesystem_nodes(id) ON UPDATE CASCADE ON DELETE RESTRICT;


--
-- Name: mdbook_preferences fk_mdbook_preferences_source_vnode_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.mdbook_preferences
    ADD CONSTRAINT fk_mdbook_preferences_source_vnode_id FOREIGN KEY (source_vnode_id) REFERENCES public.filesystem_nodes(id) ON UPDATE CASCADE ON DELETE RESTRICT;


--
-- Name: p_blog_tags fk_p_blog_tags_blog_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.p_blog_tags
    ADD CONSTRAINT fk_p_blog_tags_blog_id FOREIGN KEY (blog_id) REFERENCES public.blogs(id) ON DELETE CASCADE;


--
-- Name: p_blog_tags fk_p_blog_tags_blog_tag_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.p_blog_tags
    ADD CONSTRAINT fk_p_blog_tags_blog_tag_id FOREIGN KEY (blog_tag_id) REFERENCES public.blog_tags(id) ON DELETE CASCADE;


--
-- Name: p_website_route_references fk_p_website_route_references_db_route_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.p_website_route_references
    ADD CONSTRAINT fk_p_website_route_references_db_route_id FOREIGN KEY (db_route_id) REFERENCES public.db_routes(id) ON DELETE CASCADE;


--
-- Name: p_website_route_references fk_p_website_route_references_v_node_id; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.p_website_route_references
    ADD CONSTRAINT fk_p_website_route_references_v_node_id FOREIGN KEY (v_node_id) REFERENCES public.filesystem_nodes(id) ON DELETE CASCADE;


--
-- Name: SCHEMA public; Type: ACL; Schema: -; Owner: pg_database_owner
--

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cloudsqlsuperuser') THEN
        GRANT ALL ON SCHEMA public TO cloudsqlsuperuser;
    END IF;
END
$$;


--
-- PostgreSQL database dump complete
--

\unrestrict 1VYF7o7MsMuI8yeZyx3Sw94RnhRgxKSFIwFtwQpBR4hrur5TEFlYJEY1fBlGtt1

