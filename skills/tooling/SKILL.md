---
name: tooling
description: Select and validate applicable tool policies for a concrete operation using Host-provided capabilities. Use independently or when another task needs tool analysis.
---

# Tooling


Use only these installation resources (absolute paths, JSON string notation):

- Shared Bootstrap: {{SHARED_BOOTSTRAP}}
- Skill Root: {{SKILL_ROOT}}
- Skill CLI: {{SKILL_CLI}}
- Skill CLI --install: {{SKILL_INSTALL}}
- Installation State: {{INSTALL_STATE}}

1. Decode each path as a JSON string; use it directly as one argument or file path, independent of the current working directory. Verify the installation and required files before use; stop dependent operations if invalid.
2. Read Shared Bootstrap, then Skill Root. Discover branches on demand. The Host chooses and authorizes tools; activation grants no permission.
