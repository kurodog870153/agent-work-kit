---
name: Swagger/OpenAPI guidance
description: Use to plan API documentation contracts when the target project already has both a Swagger or OpenAPI dependency and usable configuration; otherwise not applicable.
metadata:
  tags:
    - openapi
    - swagger
---

# Swagger/OpenAPI guidance

## 0. Applicability prerequisites

1. [Required] This Reference applies only when the target project already has both a Swagger/OpenAPI dependency and usable configuration. If either is absent, the AI must not add dependencies, settings, generators or check tools, and lack of Swagger must not block a normal Controller change.
2. [Required] Once applicable, complete documentation under this file for every added or changed Controller, Endpoint method, parameter, Request Body or Response Body. Do not handle only new annotations or ignore existing gaps on the same affected Endpoint.

## 1. Complete contract

1. [Required] Every applicable Controller class and Endpoint method needs Swagger/OpenAPI annotations supported by the project's version, fully describing purpose, operation identity, tags, authorization needs and applicable deprecation semantics.
2. [Required] Annotate every path, query, header, cookie, form and multipart parameter and Request Body, fixing name, location, purpose, requiredness, type/schema, format, allowed values, range and applicable examples.
3. [Required] Annotate every possible success and error Response, fixing status, semantics, media type, schema, headers and applicable examples. Do not document only success or replace known errors with a vague default.
4. [Required] Annotate each request/response DTO directly or transitively referenced by an affected Endpoint, and each serializable field, fixing semantics, requiredness, nullability, type, format, allowed values, range, read/write mode and applicable examples. Add a DTO outside the original modification scope to the same TASK's file list and atomic change set before completing it.
5. [Required] "Complete" means every applicable property affecting contract meaning is explicit and mutually consistent; it does not require every optional annotation property irrelevant to the element.

## 2. Task-stage decisions

1. [Required] Before TASK approval, confirm the actual Swagger/OpenAPI library, version, annotation namespace, generation entry point, custom settings, contract consumers and existing documentation style. Show the evidence for user confirmation.
2. [Required] For each Endpoint, present and fix a complete contract matrix for Controller, Parameters, Request Body, all identifiable success/error Responses, DTOs and fields, including status, media type, schema and body. Obtain user confirmation before formal approval. Do not use TODO, TBD, empty descriptions, candidate values or require Execute to infer from code.
3. [Required] The Swagger contract must agree with actual validation, serialization, authorization, error handling, generics, pagination and API compatibility decisions. Resolve conflicts in TASK decisions first; do not mask behavior by editing documentation alone.
4. [Required] Generate annotations in generated code only from formal specifications, templates or generator settings. The TASK must change its single source and validate regeneration, not plan manual edits to generated files.

## 3. Validation

1. [Required] Automated VAL must actually generate or load the OpenAPI contract and confirm every applicable Endpoint, Parameter, Request Body, Response, DTO and field exists. `$ref`, schemas, statuses and media types must resolve and match the TASK matrix.
2. [Required] Use existing project contract checks, lint or consumer validation only when present and applicable. Do not add tools or external services merely to apply this instruction.
3. [Required] Human VAL confirms subjective quality such as understandable descriptions and realistic examples; automated structural success cannot replace human semantic review.
