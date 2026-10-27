---
name: Web backend resilience guidance
description: Use when planning failure and capacity boundaries for remote calls, concurrency or shared resources; not applicable without such flows.
metadata:
  tags:
    - resilience
---

# Web backend resilience guidance

1. [Required] Fix failure boundaries, maximum waits, cancellation/interruption outcomes, resource limits and partial-success semantics for remote calls, background work, batches, concurrency or shared resources. Do not use infinite timeouts or unbounded queue, collection or thread growth.
2. [Required] Allocate timeouts from an end-to-end budget across connection, read/write, resource acquisition and the whole operation. Exceptional values need environment and load evidence; do not copy arbitrary constants.
3. [Required] Retry only confirmed transient failures when the operation has idempotency, an idempotency key or safe deduplication. The TASK must fix maximum attempts, backoff, jitter, total time, retryable errors and stop conditions.
4. [Required] Do not retry validation errors, permission errors, deterministic business errors or non-idempotent operations whose outcome is uncertain but may have taken effect. Compensation requires separate authorization under the external-operations Reference.
5. [Required] Connection/thread pools, batches, pages, queues, messages, caches and memory use need observable limits and over-limit outcomes. Backpressure, rejection or degradation must match confirmed business outcomes.
6. [Required] Liveness indicates only whether the process needs a restart; readiness indicates whether it can receive new traffic; degraded status indicates limited capability still available. Do not substitute one for another or restart incorrectly for an unnecessary downstream failure.
7. [Required] Plan only the minimum logs, metrics or traces needed to diagnose each resilience mechanism. Distinguish at least normal operation, timeout, retry exhaustion, rejection, degradation and recovery without leaking sensitive data.
8. [Required] Use a circuit breaker, bulkhead, scheduler or new resilience component only if it already exists and applies or has separate approval. VAL must directly cover triggering, limits, recovery and nonretryable results.
