# Why Framework Speed Matters in the Age of AI

With the introduction of AI, software development speed has skyrocketed, allowing teams to build features faster than ever. Today, the real challenge is the speed and runtime efficiency of the framework itself. As long as a framework is open source and not closed source, it can be adapted to fit any business use case—and Lariv delivers both top-tier speed and high efficiency out of the box.

# Performance Methodology

To evaluate real-world performance, we tested all frameworks across three distinct benchmark flows using identical hosting environments and 500 concurrent worker stress tests. Each benchmark flow tests a specific layer of framework performance to provide a complete picture of speed and efficiency.

## Parsing Benchmark

This benchmark involves testing the request parsing performance of the framework. It is measuring pure framework speed by isolating HTTP request parsing and JSON data serialization without any database overhead.

[Jump to Parsing Results ↓](#parsing-benchmark-results)

## Realistic Load Benchmark

This benchmark represents what a typical, real-world application request looks like. It measures the complete end-to-end framework performance, from initial request parsing down to SQL query building, database communication, and response rendering.

[Jump to Realistic Load Results ↓](#realistic-load-benchmark-results)

## AI Workload Benchmark

We included the AI workload benchmark because modern applications frequently run heavy background tasks and AI pipelines. Its significance is measuring the framework's internal overhead and queue efficiency when running long-running processes.

[Jump to AI Workload Results ↓](#ai-workload-benchmark-results)

# Benchmark Results

Below are the detailed benchmark results and performance charts measured across 1, 50, and 500 simultaneous users.

<a id="parsing-benchmark-results"></a>

## Parsing Benchmark Results

The charts below display data parsing and serialization throughput and average response latency across different user levels.

### Parsing Test: 1 User

This measures data parsing and serialization speed for a single user processing requests in RAM. Lariv processes over 27,000 requests per second in about 0.03 milliseconds, proving its compiled engine handles request parsing far faster than other servers.

<div class="perf-pair" data-article-benchmark="counter" data-workers="1"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="counter-rps-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="counter-latency-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

### Parsing Test: 50 Users

This measures data parsing processing speed across 50 simultaneous users. Lariv exceeds 145,000 requests per second at about 0.3 milliseconds, smoothly splitting work across processor cores, while other systems stay under 1,000 requests per second and hit processing bottlenecks due to single-thread locks.

<div class="perf-pair" data-article-benchmark="counter" data-workers="50"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="counter-rps-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="counter-latency-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

### Parsing Test: 500 Users

This tests data parsing speed under heavy load with 500 simultaneous connections. Under peak traffic, Lariv sustains over 176,000 requests per second with delays under 3 milliseconds using fast modern networking, while traditional servers drop below 1,000 requests per second and slow down due to queuing.

<div class="perf-pair" data-article-benchmark="counter" data-workers="500"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="counter-rps-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="counter-latency-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

<a id="realistic-load-benchmark-results"></a>

## Realistic Load Benchmark Results

The charts below display end-to-end database request throughput and response times under typical application workloads.

### Realistic Load Test: 1 User

This test measures performance for a single user making database requests one after another. Even with just one user, Lariv runs 4 to 8 times faster than other systems, responding in about 0.74 milliseconds at over 1,340 requests per second.

<div class="perf-pair" data-article-benchmark="crud" data-workers="1"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="crud-rps-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="crud-latency-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

### Realistic Load Test: 50 Users

This test simulates a medium-sized team placing orders and viewing reports at the same time. At 50 users, other systems slow down to 110–130 milliseconds per request, while Lariv stays fast at about 18,900 requests per second with response times under 3 milliseconds.

<div class="perf-pair" data-article-benchmark="crud" data-workers="50"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="crud-rps-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="crud-latency-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

### Realistic Load Test: 500 Users

This test simulates heavy traffic during shift changes and peak business hours. At 500 users, other systems overload with delays around 1 second and large numbers of failed requests, while Lariv processes over 19,600 requests per second cleanly in about 25 milliseconds.

<div class="perf-pair" data-article-benchmark="crud" data-workers="500"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="crud-rps-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="crud-latency-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

<a id="ai-workload-benchmark-results"></a>

## AI Workload Benchmark Results

The charts below display task queue processing capacity and execution delays for background tasks and AI pipelines.

### AI Workload Test: 1 User

This measures single-user speed for sending tasks to a background queue and getting results back. Lariv processes over 13,000 tasks per second with about 0.08 milliseconds of delay, adding virtually no lag to AI workflows.

<div class="perf-pair" data-article-benchmark="task" data-workers="1"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="task-rps-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="task-latency-bars-1" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

### AI Workload Test: 50 Users

This measures background queue processing across 50 simultaneous users. Lariv handles over 66,000 tasks per second with delays under 1 millisecond, keeping background jobs running smoothly, while other systems fall to a handful of successful tasks per second.

<div class="perf-pair" data-article-benchmark="task" data-workers="50"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="task-rps-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="task-latency-bars-50" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

### AI Workload Test: 500 Users

This tests background task processing under heavy load with 500 simultaneous users. Lariv processes over 76,000 tasks per second with delays under 7 milliseconds, proving it can handle demanding AI agent pipelines. Odoo and Frappe drop to zero successful requests under the same load.

<div class="perf-pair" data-article-benchmark="task" data-workers="500"><div class="perf-card"><h4><span>Requests Per Second (RPS)</span><span class="perf-hint">Higher is better</span></h4><div id="task-rps-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div><div class="perf-card"><h4><span>Average Latency (ms)</span><span class="perf-hint">Lower is better</span></h4><div id="task-latency-bars-500" class="bars-container"><div style="color: var(--text-dark); padding: 1.5rem 0; text-align: center;">Loading benchmark metrics...</div></div></div></div>

<p class="article-source"><a href="https://raw.githubusercontent.com/UniquityVentures/benchmarks/refs/heads/main/benchmark_metrics.json" target="_blank" rel="noopener">Data sourced from GitHub: UniquityVentures/benchmarks ↗</a></p>

## Why User Scale Matters

Traditional ERP systems hit a hard performance limit around 400–500 requests per second, causing user waiting times to jump from tens of milliseconds to around 1 second, with large numbers of failed requests. Lariv avoids these delays by separating network connections from internal processing, sustaining over 19,600 requests per second at about 25 milliseconds during peak traffic.

## Choosing the Right Framework

Choosing an application framework is an important business decision, and core speed determines how reliably it runs as your company grows. While other systems like Odoo and Frappe slow down under heavy traffic, Lariv handles high loads with fast speeds, low delays, and zero dropped requests, keeping server costs low and work moving quickly.

## Potential Cost Benefits

Performance directly impacts server bills and infrastructure costs. Choosing Lariv over traditional frameworks offers substantial financial advantages:

- **Up to 80–90% Reduction in Infrastructure Spending:** Because Lariv handles over 19,600 requests per second compared to ~400–500 RPS on traditional frameworks like Odoo or Frappe, a single server running Lariv can process workloads that would normally require a cluster of multiple application servers behind a load balancer.
- **Lower Memory & CPU Footprint:** Traditional frameworks rely on multi-process workers that consume gigabytes of RAM under concurrent traffic. Lariv's compiled engine handles thousands of simultaneous connections with minimal memory and CPU consumption.
- **Reduced Maintenance and Operations:** Fewer active servers mean simplified deployment pipelines, lower cloud monitoring overhead, fewer points of system failure, and reduced DevOps operational costs.
- **Longer Hardware Lifetime:** As your organization grows, Lariv scales effortlessly on existing hosting tiers, deferring the need for expensive infrastructure upgrades and dedicated cloud instances.

### Try the Benchmark Code Yourself

All benchmark scripts, test data, and setup steps are open source so anyone can test and verify our results. You can view the code and run the tests on your own servers by visiting the [Lariv Benchmarks GitHub Repository](https://github.com/UniquityVentures/benchmarks).
