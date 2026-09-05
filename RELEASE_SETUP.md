# Release Setup — Required GitHub Configuration and External Controls

## Current workflow state

The `.github/workflows/release.yml` is a **read-only build pipeline**. It
validates an immutable tag, runs fmt/clippy/test, builds universal binary +
source archives, generates SHA-256 checksums, and uploads artifacts via
`actions/upload-artifact`. There is **no `contents: write` job, token, or
GitHub Release creation** anywhere in the workflow.

Manual GitHub Release publication (downloading artifacts from the Actions
run and creating the release by hand after review) is the current process.

## Prerequisites for automated publication

Before adding any automated publish job (which would need `contents: write`),
**all** of the following must be configured and independently verified:

### 1. Protected `release` environment
- Create: Settings → Environments → New environment → `release`
- Required reviewers: ≥1 independent reviewer (no self-approval)
- Deployment branches: restrict to `main`

### 2. Environment-scoped sentinel secret
- Add secret `RELEASE_AUTHORIZATION` to the `release` environment (any
  non-empty value).
- **Critical:** this must be an **environment** secret, NOT a repository or
  organization secret. A same-named repo/org secret would resolve regardless
  of environment protection, defeating the fail-closed gate.
- Verify: `gh api repos/<owner>/<repo>/environments` lists `release`;
  confirm the secret exists only at the environment level.

### 3. Strict `main` branch protection
- Require pull requests: yes, ≥1 approval
- Require status checks: Format, Clippy, Test — strict + up-to-date
- Require conversation resolution
- Enforce admins, restrict bypass

### 4. Mandatory immutable `v*` tag ruleset
- Restrict tag creation, update, and deletion to admins (or a dedicated role)
- Preferably require signed tags
- This is the **strong control** that prevents tag movement between build
  and publish; the workflow revalidates the SHA but only tag immutability
  eliminates the race entirely

### 5. Actions policy
- Allow only reviewed actions
- Require full commit SHA pins

### 6. Credential incident resolution
- The exposed API credential must be revoked/rotated
- Removed from inherited launchd/shell state
- Incident owner attests completion before any release dispatch

## Verification

```bash
gh api repos/<owner>/<repo>/environments          # must list "release"
gh api repos/<owner>/<repo>/branches/main/protection
gh api repos/<owner>/<repo>/rulesets              # must list the v* tag ruleset
```

Until all six prerequisites are verified, **do not add a publish job**.
The current read-only build pipeline is safe to run without them.
