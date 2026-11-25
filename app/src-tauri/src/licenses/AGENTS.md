# License filter rules

- The license filter (Features, default OFF) reads only VN3, UV License and CC0 into
  a separate, deletable `licenses.sqlite3` (schema 1).
- A failed or partial read never replaces a result or settles as none, and a user's
  edit always wins.
- CC0 needs a sentence declaring the asset itself CC0: a mishit is worse than a miss.
- A 0-yen license file is read with an account that holds the asset, which adds a
  free-history row; a bought asset's free row never lists it under Free.
