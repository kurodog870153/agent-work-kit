# Report Tooling Policy Failure

1. Stop only operations that depend on an invalid node or Reference; root or shared ancestor failure affects all their dependents. An unaffected operation may continue with its own valid sources.
2. Report the operation, target, applicable source paths, raw and applicable canonical identities, observed failure and affected scope. Unknown source or tool state remains unknown until verified.
3. The Host may use another tool only when it still satisfies every required policy, evidence and authorization condition. Do not retry an operation with uncertain side effects or claim an automatic rollback.
