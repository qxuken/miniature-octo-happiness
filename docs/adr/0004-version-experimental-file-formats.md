# Version the experimental file format without freezing it

Include an explicit engine file-format version and reject unsupported versions,
but allow breaking format changes during the learning milestones. This permits
storage experiments without pretending earlier files remain compatible; tools
to preserve and convert experimental files are not yet promised. Record schema
versions and application data migrations are separate concerns and must not be
encoded as changes to the engine format version.
