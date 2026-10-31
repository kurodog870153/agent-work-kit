# Discover and recommend external skills


1. The Host discovers available skills from its enabled Skill roots and decides which are relevant to the request. The retired `skills catalog` command is not a discovery gate.
2. Treat skills with identical names but different scope, root, or source as distinct identities.
3. Check dependencies before recommendation. Mark unavailable skills as unselectable; never install packages or connect services automatically.
4. The Host recommends the smallest set needed by the request. Show identity, description, recommendation reason, source and scope, mode support, dependency status, invocation policy, and estimated context cost.
5. Explicit-only skills may be recommended. Record the user's confirmation in the formal selection; Host loading still follows the Host's own permissions and current request.
6. Ask the user to accept, add, remove, or cancel. If no suitable skill exists, ask whether to continue `base_only`.
7. After the Host has confirmed choices, use `skills snapshot` and `skills selection-build/selection-validate` to verify the selected identities, source bytes, mode support and fingerprints for formal TASK use. Stop the dependent formal operation on drift.
