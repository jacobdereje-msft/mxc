# Process Container Networking Configuration, GA

Starting with schema 0.8.0, ProcessContainer networking uses the shared `network.egress` and `network.ingress` policy
plus `runtimeConfig.networkProxy` and `processContainer.network.allowedProxyPeer`.

Implementation companion to the parent [MXC Network Configuration, GA](../sandbox-policy/v2/networking.md) doc, which owns the shared policy schema, the three connectivity models, and the GA goal (model 2, deny-all-except-proxy). This doc covers only how the Windows processcontainer backend enforces those models.

## 1. What this backend delivers at GA

Each sandbox gets two enforcement primitives, scoped to its container SID and applied with no UAC prompt per launch:

- **WFP internet filters:** block internet-bound traffic by default, then allow or block specific public destinations by
  IP address or range, protocol, and port for both IPv4 and IPv6. An explicit block always wins over an allow. The rules
  apply only to this sandbox.
- **Per-container WinHTTP HTTP/S proxy:** points WinHTTP-stack clients at a caller-provided AppContainer proxy. MXC
  also sets the standard proxy environment variables for other HTTP libraries. These settings are a compatibility
  layer for well-behaved clients; the containment boundary is the absence of direct internet capability. Private
  network traffic remains available in both directions when `ingress.default` is `"allow"`.

The examples below use the proposed schema 0.8 network shape.

Windows exposes `privateNetworkClientServer` as one bidirectional AppContainer capability. ProcessContainer therefore
maps `ingress.default: "allow"` to private-network communication in both directions. `egress` controls internet-bound
traffic and does not independently narrow private-network client connections. `ingress.hostLoopback` remains the
separate host-loopback control.

### Model 1: direct egress, WFP-filtered (least restrictive)

- **Capabilities:** `internetClient`, plus `privateNetworkClientServer` only when `ingress.default` is `"allow"`.
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
| Client capability | `privateNetworkClientServer`, enabled by `ingress.default: "allow"` |
| Proxy capabilities | `privateNetworkClientServer`; also `internetClient` for external destinations |
| Peer | Package family name or AppContainer profile name in `allowedProxyPeer` |
| Enforcement | Internet blocked; private network is bidirectional |

This is a ProcessContainer-specific mapping. Callers that need a private-network proxy or any other private-network
communication must set `ingress.default` to `"allow"` and accept that Windows enables both private-network client and
server behavior. WFP continues to enforce the separate internet-bound `egress` policy.

#### Contained AppContainer proxy (recommended)

Use the canonical
[ProcessContainer schema 0.8 configuration](examples/0.8.0-schema.md), which
shows `runtimeConfig.networkProxy`,
`processContainer.network.allowedProxyPeer`, and their relationship in one
place.

#### HTTP client guidance

Code inside the ProcessContainer should use WinHTTP or an HTTP library that queries the system for proxy information.
The OS sets the proxy configuration per BaseContainer, and the WinHTTP stack uses it transparently. The proxy process
itself does not use this per-BaseContainer configuration.

MXC also sets the standard proxy environment variables for libraries that use cooperative proxying. Direct internet
traffic that bypasses the proxy is blocked. Other private-network traffic remains available because model 2 requires
the bidirectional private-network capability.

Model 2 requires `egress.default: "deny"`, `ingress.default: "allow"`, and
`ingress.hostLoopback: "deny"`. The private-network allow is required for the AppContainer proxy path and is
bidirectional. Proxy mode cannot contain direct egress allow or deny rules.

The proxy endpoint is runtime metadata, not shared network policy. MXC resolves `allowedProxyPeer`, configures the
per-container WinHTTP proxy, and grants the private-network capability selected by `ingress.default`.

The caller creates and authorizes the proxy, starts it before the BaseContainer, keeps it alive until the client exits,
and leaves egress deny-default with no direct allow or deny rules.

### Model 3: fully blocked (most restrictive)

- **Capabilities:** none; no loopback exemptions.
- **Enforcement:** no proxy; all outbound and inbound dropped.

When no runtime proxy or backend proxy peer is configured, deny-all is the
default and model 3 is also the result of providing no network policy at all:
the explicit form, an omitted network block, and an empty `"network": {}` are
equivalent:

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

`Experimental_CreateProcessInSandbox` (CPIS) could be different between builds
as the network-policy surface grows over time. A machine can expose the API but
not yet honor a specific policy field MXC asks for. MXC must not silently fall
back to Tier 2 in that case: the two paths have different security and cleanup
properties, and the operator would not know. The contract:

- Fall back to Tier 2 only when the API is absent on the build, not when it is present but missing a requested field.
- For a present-but-incomplete API, MXC rejects the launch with a typed error naming the missing capability.

## 3. WFP is the enforcement primitive (both tiers)

**Admin requirement.** Adding WFP filters is admin-only. On Tier 1 the OS applies them in its own elevated context; on Tier 2 (Windows 23H2) MXC elevates on each launch to write the filters.

**Cleanup.** Filters will need to have a lifetime ≤ sandbox lifetime. In both tiers the filters will need to be cleaned up when there are no more processes running in the container.

`internetClient` and WFP implement the internet-bound `egress` policy.
`privateNetworkClientServer` is a separate, intentionally bidirectional Windows capability selected through
`ingress.default`. ProcessContainer cannot represent independent private-network outbound and inbound controls.
