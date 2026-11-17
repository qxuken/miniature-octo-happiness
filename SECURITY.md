# Security reporting

Report suspected data loss, memory unsafety, authorization failures or other
security defects privately to **sec@qxuken.dev**. This is also the release-owner
contact. Include the source revision, build identity, platform/filesystem,
feature set and a minimal reproduction when possible. Avoid sending database
contents, credentials, private keys or personal records in an initial report.
Ask for a suitable transfer method if sensitive evidence is necessary.

The maintainer triages reports, preserves a regression input or fault schedule,
and verifies a fix against the affected release profiles before distributing
a private replacement. A confirmed durability defect requires documenting
affected revisions, detection, backup/restore guidance and any acknowledged
data that may be at risk. Public disclosure is coordinated with the reporter.
There is no guaranteed response or patch deadline during pre-release work.

No public production release is currently qualified. The project name, version
and Rust API remain provisional. Cargo packages have `publish = false`; this
repository does not authorize registry publication. Qualification status and
remaining gates are tracked in [docs/roadmap-status.md](docs/roadmap-status.md).

For a qualified private release, the release owner records the supported source
revision, feature profiles and OS/filesystem matrix with its evidence bundle.
Security fixes target that supported revision; other experimental revisions
require reproduction or upgrade. Known data-loss, UB or authorization defects
within the supported profile block qualification. Release notes describe format
and protocol changes, upgrade/rollback steps and any remaining limitations.
