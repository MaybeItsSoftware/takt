# Code Signing, Notarization, and Distribution

> The Xcode project this page describes was retired with the Swift Mac app on
> 2026-11-08. The Mac app is now signed by `scripts/install_desktop.sh` (the
> first valid Apple Development identity, else ad hoc; `TAKT_SIGN_IDENTITY`
> chooses) and distributed by `scripts/build_desktop_dmg.sh`, which uses the
> same Developer ID certificate and `priority-notary` profile described below.

Team: **DEMOPOL LABS LIMITED** — Team ID `6NQNU5YSC2`.

## What is configured

Both configurations sign with a real Apple-issued identity rather than the
ad-hoc `-` ("Sign to Run Locally") identity the project used previously:

| Setting | Debug | Release |
|---|---|---|
| `CODE_SIGN_IDENTITY[sdk=macosx*]` | `Apple Development` | `Apple Development` |
| `CODE_SIGN_STYLE` | `Manual` | `Manual` |
| `DEVELOPMENT_TEAM` | `6NQNU5YSC2` | `6NQNU5YSC2` |
| `ENABLE_APP_SANDBOX` | `NO` | `NO` |
| `ENABLE_HARDENED_RUNTIME` | — | `YES` |
| Entitlements | `Takt/Takt.entitlements` | `Takt/Takt.release.entitlements` |

No provisioning profile is needed. The app's entitlements — user-selected
files, network client/server — are all profile-free on macOS, so
`PROVISIONING_PROFILE_REQUIRED` resolves to `NO`. Adding a profile-backed
capability later (app groups, iCloud, push) changes that.

The sandbox is deliberately **off** in both configurations, and
`.github/workflows/ci.yml` fails the build if either the release entitlements
or `ENABLE_APP_SANDBOX` turn it back on. Sandboxing is only mandatory for Mac
App Store distribution; this app ships as a notarized DMG, which does not need
it. It also cannot simply be re-enabled: under the sandbox, keychain access is
scoped to an access group that comes from `com.apple.application-identifier`,
which only an embedded provisioning profile grants, and the project signs
manually with none — so the sandboxed build had no access group, every
`SecItemAdd` failed, and the app silently forgot the Checkvist remote key on
every relaunch. Turning it on again means provisioning a profile and adding
`keychain-access-groups` first; the long comment in
`Takt/Takt.release.entitlements` is the authoritative account.

### Why a real identity rather than ad-hoc

An ad-hoc signature's designated requirement is a `cdhash` — the literal hash
of the binary — so it changes on **every rebuild**. Anything keyed to the app's
identity therefore treats each build as a different application. In practice
that meant macOS re-prompting for keychain access after every reinstall, which
is the reason `ignoreKeychainInDebug` exists.

Signing with the team certificate makes the designated requirement
`identifier "uk.co.maybeitssoftware.takt" and ... certificate leaf[subject.OU] = "6NQNU5YSC2"`,
which is stable across rebuilds. The keychain entry survives.

> The switch from ad-hoc to the team certificate changes the identity **once**.
> Expect a single keychain prompt on the first launch after upgrading, then
> silence.

### CI

CI has no certificates in its keychain, so both build steps in
`.github/workflows/ci.yml` pass `CODE_SIGNING_ALLOWED=NO`. Keep that flag on any
new `xcodebuild` step you add there.

## Distribution: two things are still missing

`scripts/build_desktop_dmg.sh` handles signing and notarization automatically, but only
once these exist. Until then it prints a warning and produces a DMG that works
on this machine and nowhere else. **Neither step can be done from the command
line** — both need a human with Account Holder access.

### 1. Create a "Developer ID Application" certificate

The installed certificates are `Apple Distribution` and
`3rd Party Mac Developer Installer` — those are for the **Mac App Store**. A DMG
distributed from a website needs `Developer ID Application`, which is a
different certificate type and is not present.

Xcode → Settings → Accounts → sign in → select the DEMOPOL LABS team →
**Manage Certificates…** → **+** → **Developer ID Application**.

Only the Account Holder can create one, and there is a hard limit of five per
team, so check the [certificates list](https://developer.apple.com/account/resources/certificates/list)
before creating another.

### 2. Store notarization credentials

```bash
xcrun notarytool store-credentials "priority-notary" \
  --apple-id "<your-apple-id>" \
  --team-id "6NQNU5YSC2" \
  --password "<app-specific-password>"
```

The password is an **app-specific password** from
[account.apple.com](https://account.apple.com) → Sign-In and Security →
App-Specific Passwords. Your normal Apple ID password will not work.

Override the profile name with `NOTARY_PROFILE=<name>` if you use a different
one.

### Then

```bash
./scripts/build_desktop_dmg.sh 2.2.0
```

The script signs the app with Developer ID, signs the DMG, submits it to Apple,
waits for the result, and staples the ticket so the app launches without a
network round trip.

## Verifying a build

```bash
# Which identity signed it, and is the signature intact?
codesign -dv --verbose=4 "/Applications/Takt.app" 2>&1 | grep -E 'Authority|Identifier|flags'
codesign --verify --deep --strict "/Applications/Takt.app"

# Entitlements actually baked into the bundle
codesign -d --entitlements - --xml "/Applications/Takt.app" | plutil -p -

# Would Gatekeeper let this run on someone else's Mac?
spctl --assess --type execute --verbose "/Applications/Takt.app"
```

`spctl` reporting `rejected` or `source=Unnotarized Developer ID` means the
notarization step above has not been completed.
