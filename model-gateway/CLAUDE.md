# Claude

## Guideline

- If text contains dynamic pattern, `#variableName#` (starts end with `#`)
  will be used as example `#entityName#IdRequest`

## Code

- Comment top-level function / methods in rust doc format,
  except local block which comment in the following format:
  ```
  // * lowercase only comment expect `VaribaleName` in backticks`
  ```
