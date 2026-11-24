---
name: Work root
description: Applies to any Work activity that creates, reads or changes formal Source, Discussion, TASK, Execution or Acceptance records; not applicable to work that keeps no formal records.
metadata:
  tags:
    - work-records
---

# Work root

1. [Required] Formal Source, Discussion, TASK, Execution, Attempt, Acceptance and Correction records are the only authority for state and history. Read and write them only through the Work CLI; never edit formal files by hand.
2. [Required] Persist confirmed requirements, decisions and work records in formal records. A technology, Skill or standard that is a formal requirement, constraint or confirmed decision must be recorded and must not be changed through dynamic instruction selection.
3. [Required] Never change a confirmed requirement, decision or acceptance condition without user confirmation. A change of context sources does not itself revise records; when it affects requirements, decisions, acceptance or results, review, approve and revise the dependent records.
4. [Required] The user's decision, the Host's permission and the approval of a formal candidate are independent. Bind approval to the candidate actually reviewed; an unapproved, invalid, conflicting or incomplete operation is not a success.
5. [Required] Published history and original evidence are immutable. Handle a mistake in a current record with a legal revision or a Correction. Never normalize historical bytes, forge approval or mark an unverified result as passed.
6. [Required] Work validates format, relations, versions, integrity, legal transitions and retention; it does not judge the truth of external reports, tool authorization or semantic acceptance. Accept only records of the current contract and offer no data migration.
7. [Required] For every Preview (including TASK and Correction), save one JSON response at `.work/preview/{record-kind}/{record-id}.json` under the project Work root; replace the previous candidate on re-preview, do not reuse approval for a changed candidate or save a duplicate text view. Delete it only after confirmed Apply success; retain it on failure or unknown outcome and inspect formal state before retrying. Preview files are Host-managed, not formal records.
8. [Required] Do not modify or delete Work CLI's internal lock or temporary write file.
9. [Default] Choose operations from the need and the CLI's Help and contract. A CLI status query never decides the next step.
