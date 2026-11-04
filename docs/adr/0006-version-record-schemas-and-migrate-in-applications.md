# Version record schemas and keep migrations application-side

Support optional immutable schema descriptors with stable field IDs in a
stored catalog, while generic maps and lists remain usable without schemas.
Applications provide programmatic record transformations because changes of
meaning cannot be inferred safely from native struct definitions. Keep these
record migrations separate from the engine's versioned file format; schema
validation belongs to typed wrappers or explicit application code while the
core validates representation integrity. Raw collection writes can bypass
schema checks; storing schema metadata is not by itself a guarantee of record
validity or an automatically maintained index.
