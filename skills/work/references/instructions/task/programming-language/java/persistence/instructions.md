---
name: Java persistence task planning
description: Use to fix Java relational data-access technology and common contracts; not applicable to work outside the persistence layer.
metadata:
  tags:
    - persistence
    - relational-data
---

# Java persistence task planning

Instruction boundary: This layer only fixes persistence-technology selection and common data contracts; child layers own JPA and MyBatis details.

## 1. Reference routing

1. [Required] Evaluate the relational-data Reference below and load it only when its condition applies.

## 2. Technology selection

1. [Required] When JPA or MyBatis/MyBatis-Plus is used, load the corresponding child layer for each actually affected flow. If both apply, preserve their separate boundaries; do not mix APIs or assume only one can be selected.
2. [Required] For relational data, cross-check dependencies with the target flow's annotations, interfaces, configuration, queries or mappings to identify JPA, MyBatis or MyBatis-Plus. Follow the actual chain for an existing flow. If several options equally apply to a new flow, show evidence and have the user choose; do not leave candidates for Execute.
3. [Required] Technology selection does not authorize new or upgraded dependencies, processors, plugins, settings or migrations. One indivisible TASK may cover several existing technology branches, but must fix each flow boundary and instruction layer separately.
4. [Required] Cross-check build dependencies against actual configuration, annotations, interfaces, implementation, queries or mappings to identify relational technology. Do not decide from a parent module, BOM, starter, transitive dependency or unused code alone. For JPA, check at least the Persistence namespace, applicable Spring Data or Hibernate dependency, and Entity/Repository/EntityManager/queries. For MyBatis, check at least dependencies and Mapper/XML/SqlSession/BaseMapper/actual queries.
5. [Required] Select existing flows from their actual data-access chains; technology elsewhere in the repository cannot override them. If several options equally apply to a new flow, neither is used, or evidence conflicts, show the evidence and have the user specify the choice. For each affected flow, the formal TASK fixes technology, module, evidence, actual version, namespace/provider and data architecture; do not leave this to Execute.

## References

| Reference | Load condition |
| --- | --- |
| `references/relational-data.md` | The TASK involves a relational database, schema, migration, SQL, transaction, index or data mapping. |
