# Process Container Networking Configuration, GA

Starting with schema 0.8.0, ProcessContainer networking uses the shared
`network.egress` and `network.ingress` policy plus
`runtimeConfig.networkProxy` and
`processContainer.network.allowedProxyPeer`. Schema 0.7 and earlier retain the
legacy network shape and compatibility behavior described below.

Implementation companion to the parent [MXC Network Configuration, GA](../sandbox-policy/v2/networking.md) doc, which owns the shared policy schema, the three connectivity models, and the GA goal (model 2, deny-all-except-proxy). This doc covers only how the Windows processcontainer backend enforces those models.

| Schema version | Config shape | Proxy setup behavior |
|---|---|---|
| 0.7.0 and earlier | Existing `network.proxy` shape remains unchanged | Compatibility path injects the cooperative proxy variables; it does not use the 0.8 proxy-peer contract |
| 0.8.0 and later | Adds `egress`/`ingress`, `runtimeConfig.networkProxy`, and optional `processContainer.network.allowedProxyPeer` | Uses a packaged or unpackaged AppContainer proxy peer when named; otherwise requires explicit host-loopback opt-in |

Schema 0.7 does not adopt the 0.8 package/AppContainer enforcement model. It
keeps the legacy cooperative proxy configuration, including
`HTTP_PROXY`/`HTTPS_PROXY` injection for clients that honor those variables.
Schema 0.8 introduces the enforced proxy-peer path and its explicit
host-loopback alternative.

See the ProcessContainer examples for complete
[schema 0.7](examples/0.7.0-schema.md) and
[schema 0.8](examples/0.8.0-schema.md) configurations.

## 1. What this backend delivers at GA

Each sandbox gets two enforcement primitives, scoped to its container SID and applied with no UAC prompt per launch:

- **WFP outbound filters:** block all outbound traffic by default, then allow or block specific destinations by IP address or range, protocol, and port (a single port or a range), for both IPv4 and IPv6. An explicit block always wins over an allow, so a deny is expected to fall inside the allow it narrows; an allow and a deny matching the exact same destination, protocol, and port is rejected as an invalid policy. The rules apply only to this sandbox.
- **Per-container WinHTTP HTTP/S proxy:** points WinHTTP-stack clients (e.g., the WinHTTP/Chromium stack) at a caller-provided loopback proxy container. MXC also sets the proxy env vars (`HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY`, plus lowercase versions) to the same loopback endpoint. Runtimes that read those variables rather than WinHTTP (Node tooling, Python `requests` / `pip`, Go `net/http`, `curl`, `git`) route through the proxy using this mechanism. These variables are a compatibility layer for well-behaved clients, not the containment boundary. All traffic not destined for the proxy loopback will be dropped.

Each model is a specific combination of container network capabilities and
enforcement. Complete schema 0.8 configs and proxy-host setup instructions are
shown below. These examples are forward-looking until the schema 0.8
networking implementation lands.

ProcessContainer ingress intentionally has no peer or port rules.
`ingress.default` controls LAN/private-network inbound traffic, and
`ingress.hostLoopback` separately controls host-loopback connectivity in
either direction. The `hostLoopback` value overrides `default` for that path.
WAN inbound is outside the GA policy and remains blocked.

### Model 1: direct egress, WFP-filtered (least restrictive)

- **Capabilities:** internetClient, plus a loopback exemption for same-container connections; no other network capability.
- **Enforcement:** WFP allow/block rules; no proxy.

```jsonc
{
  "network": {
    "egress": {
      "default": "deny",
      "allow": [
        { "to": [ { "cidr": "140.82.112.0/20" } ],
          "ports": [ { "protocol": "tcp", "port": 443 } ] }
      ]
    },
    "ingress": {
      "default": "deny",
      "hostLoopback": "deny"
    }
    // direct egress, filtered by WFP
  }
}
```

### Model 2: proxy-only egress (recommended)

| Item | Requirement |
|---|---|
| BaseContainer capability | `privateNetworkClientServer` |
| Proxy capabilities | `privateNetworkClientServer`; also `internetClient` for external destinations |
| Peer | Package family name or AppContainer profile name in `allowedProxyPeer`, or no peer when opting into a host proxy |
| Enforcement | Per-container WinHTTP proxy plus scoped loopback; all direct egress remains blocked |

#### Contained AppContainer proxy (recommended)

```jsonc
{
  "runtimeConfig": { // MXC runtime metadata (not policy)
    "networkProxy": "http://127.0.0.1:8080"
  },
  "processContainer": {
    "network": {
      // Package family name or AppContainer profile name of the proxy.
      "allowedProxyPeer": "agent-proxy"
    }
  }
}
```

The omitted `network` block uses the default-deny posture. An explicit block
with `egress.default: "deny"`, `ingress.default: "deny"`, and
`ingress.hostLoopback: "deny"` is equivalent. Proxy mode cannot contain direct
egress allow or deny rules.

The proxy endpoint is runtime metadata, not shared network policy. MXC:

- resolves `allowedProxyPeer` and creates the scoped loopback relationship;
- adds `privateNetworkClientServer` to the BaseContainer client; and
- configures the per-container WinHTTP proxy.

The caller must:

- create and authorize the proxy;
- start it before the BaseContainer;
- keep it alive until the client exits; and
- leave egress deny-default with no direct allow or deny rules.

#### Proxy identity and firewall authorization

| Proxy setup | `allowedProxyPeer` | Firewall authorization |
|---|---|---|
| Packaged AppContainer | Package family name | Package-owned `desktop2:Extension Category="windows.firewallRules"` inbound TCP rule |
| Unpackaged AppContainer | [AppContainer profile](https://learn.microsoft.com/windows/win32/api/userenv/nf-userenv-createappcontainerprofile) name | Administrator-installed inbound application rule for the proxy executable |

The scoped peer rule and `privateNetworkClientServer` do not bypass Windows
Firewall's block-inbound-to-non-allowed-apps policy. A packaged proxy needs
AppContainer trust and the package-owned firewall declaration shown in the
stacked follow-up. An unpackaged AppContainer proxy requires its installer or
administrator to own the equivalent firewall rule.

#### Host-process proxy (explicit opt-in)

If the proxy does not run in an AppContainer, it is a host process.
Omit `processContainer.network.allowedProxyPeer` and explicitly allow the host
loopback path:

```jsonc
{
  "network": {
    "egress": { "default": "deny" },
    "ingress": {
      "default": "deny",
      "hostLoopback": "allow"
    }
  },
  "runtimeConfig": {
    "networkProxy": "http://127.0.0.1:8080"
  }
}
```

This is a deliberate relaxation from the packaged peer model:
`hostLoopback: "allow"` permits the sandbox to use the host-loopback path, and
there is no contained AppContainer identity to scope with `allowedProxyPeer`.
Direct internet egress remains deny-default.

### Model 3: fully blocked (most restrictive)

- **Capabilities:** none; no loopback exemptions.
- **Enforcement:** no proxy; all outbound and inbound dropped.

Since deny-all is the default, model 3 is also the result of providing no network policy at all: the explicit form, an omitted network block, and an empty `"network": {}` are equivalent:

```jsonc
// explicit (canonical blocked: direct egress, default deny, no allow rules)
{
  "network": {
    "egress": { "default": "deny" },   // no allow rules
    "ingress": {
      "default": "deny",
      "hostLoopback": "deny"
    }
  }
}

// or
{ /* no "network" key at all */ }

// or
{ "network": {} }
```

### 1.1 Out of GA scope for this backend

Do not infer otherwise from the schema:

- Transparent TCP/UDP redirection through the proxy. GA proxying is WinHTTP HTTP/S only.
- L7 classification (e.g., HTTPS vs SSH on :443).
- Durable DNS-name rules.
- Encrypted-payload inspection.
- Per-source or per-port inbound rules. GA ingress is limited to the
  `default` and `hostLoopback` allow/deny toggles.

See the parent doc on the last 4.

## 2. Two enforcement paths: current vs downlevel

Both (a) WFP filter writes and (b) per-container WinHTTP proxy configuration require a privileged context. How that privilege is obtained is the entire implementation story for this backend, and it splits by Windows build:

| Tier 1: the OS applies the policy in-process | Tier 2: downlevel (Windows 23H2) |
|---|---|
| On builds that expose the OS sandbox-creation API (`CreateProcessInSandbox`), the OS itself, in its own elevated context, applies the per-sandbox WFP filters and wires the WinHTTP proxy before the target process runs.<br><br>No MXC-side privileged component, no UAC. The filter lifetime is owned by the OS and bound to AppContainer. This is the preferred path and where new capabilities land first. | On builds without that API (Windows 23H2), only model 1 (direct egress, WFP-filtered) is supported. MXC applies the per-sandbox WFP filters by elevating on each launch to write them.<br><br>There is no per-container WinHTTP proxy support on 23H2, so model 2 (proxy-only egress) is available only on builds that expose `CreateProcessInSandbox`. |

### 2.1 Fail loud on version skew: never silently downgrade

`CreateProcessInSandbox` could be different between builds as the network-policy surface grows over time. A machine can expose the API but not yet honor a specific policy field MXC asks for. MXC must not silently fall back to Tier 2 in that case: the two paths have different security and cleanup properties, and the operator would not know. The contract:

- Fall back to Tier 2 only when the API is absent on the build, not when it is present but missing a requested field.
- For a present-but-incomplete API, MXC rejects the launch with a typed error naming the missing capability.

## 3. WFP is the enforcement primitive (both tiers)

AppContainers today have 3 network capabilities: `internetClient`, `internetClientServer`, and `privateNetworkClientServer`. Direct egress uses `internetClient` as an on/off switch for outbound internet connectivity. Proxy mode instead gives both the BaseContainer client and AppContainer proxy server `privateNetworkClientServer`; only the proxy receives `internetClient`. Beyond that, outbound policy is enforced with the Windows Filtering Platform (WFP), the OS's built-in network-filtering engine. When the sandbox tries to open an outbound connection, the kernel checks MXC's filters and allows or blocks it. Each filter is scoped to the sandbox's container SID, so it applies only to that sandbox.

**Admin requirement.** Adding WFP filters is admin-only. On Tier 1 the OS applies them in its own elevated context; on Tier 2 (Windows 23H2) MXC elevates on each launch to write the filters.

**Cleanup.** Filters will need to have a lifetime ≤ sandbox lifetime. In both tiers the filters will need to be cleaned up when there are no more processes running in the container.
