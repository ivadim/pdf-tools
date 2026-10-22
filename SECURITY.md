# Security policy

## Supported versions

Only the [latest release](https://github.com/ivadim/pdf-tools/releases/latest) and the web version at [ivadim.github.io/pdf-tools](https://ivadim.github.io/pdf-tools/) get security fixes.

## Reporting a vulnerability

Please **don't open a public issue**. Report it privately through [GitHub's vulnerability reporting](https://github.com/ivadim/pdf-tools/security/advisories/new) (*Security → Report a vulnerability*).

Include what you can of:

- the version (or commit) and system,
- steps to reproduce, ideally with a PDF that triggers it,
- what an attacker could achieve.

You should get a reply within a week. Once a fix is released, the advisory is published with credit to you, unless you'd rather stay anonymous.

PDF Tools opens untrusted files, so crashes, hangs or excessive memory use on a crafted PDF are in scope.

## Verifying downloads

Releases after 0.3.0 include a `SHA256SUMS` file and a signed build-provenance attestation, which proves a file was built by this repository's CI from the tagged commit:

```bash
gh attestation verify <downloaded file> --repo ivadim/pdf-tools
```

The attestation is also attached to the release as `pdftools-<version>.intoto.jsonl`, for checking offline with `--bundle`.
