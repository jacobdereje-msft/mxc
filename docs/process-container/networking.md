# Process Container Networking Configuration, GA

Starting with schema 0.8.0, ProcessContainer networking uses the shared `network.egress` and `network.ingress` policy
plus `runtimeConfig.networkProxy` and `processContainer.network.allowedProxyPeer`.

Implementation companion to the parent [MXC Network Configuration, GA](../sandbox-policy/v2/networking.md) doc, which owns the shared policy schema, the three connectivity models, and the GA goal (model 2, deny-all-except-proxy). This doc covers only how the Windows processcontainer backend enforces those models.

## 1. What this backend delivers at GA

Each sandbox gets two enforcement primitives, scoped to its container SID and applied with no UAC prompt per launch:

- **WFP outbound filters:** block all outbound traffic by default, then allow or block specific destinations by IP address or range, protocol, and port (a single port or a range), for both IPv4 and IPv6. An explicit block always wins over an allow, so a deny is expected to fall inside the allow it narrows; an allow and a deny matching the exact same destination, protocol, and port is rejected as an invalid policy. The rules apply only to this sandbox.
- **Per-container WinHTTP HTTP/S proxy:** points WinHTTP-stack clients (e.g., the WinHTTP/Chromium stack) at a caller-provided loopback proxy container. MXC also sets the proxy env vars (`HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY`, plus lowercase versions) to the same loopback endpoint. Runtimes that read those variables rather than WinHTTP (Node tooling, Python `requests` / `pip`, Go `net/http`, `curl`, `git`) route through the proxy using this mechanism. These variables are a compatibility layer for well-behaved clients, not the containment boundary. All traffic not destined for the proxy loopback will be dropped.

The examples below use the proposed schema 0.8 network shape.

ProcessContainer ingress has no peer or port rules. `ingress.default` controls
LAN/private-network inbound traffic; `ingress.hostLoopback` controls inbound
host-loopback-to-sandbox traffic and overrides `default` for that path. WAN
inbound remains blocked.

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
| Client authorization | Exact outbound path to the configured proxy endpoint |
| Proxy capabilities | `privateNetworkClientServer`; also `internetClient` for external destinations |
| Peer | Package family or AppContainer profile; omit for a host-process proxy |
| Enforcement | Per-container WinHTTP proxy plus endpoint-scoped outbound authorization; all other egress and policy-denied ingress remain blocked |

Windows capabilities and loopback exemptions are backend implementation
details, not policy. If the backend needs `privateNetworkClientServer` to reach
the proxy, WFP and firewall rules must narrow that capability to the requested
outbound endpoint and preserve the configured ingress policy.

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

#### HTTP client guidance

Code inside the ProcessContainer should use WinHTTP or an HTTP library that queries the system for proxy information.
The OS sets the proxy configuration per BaseContainer, and the WinHTTP stack uses it transparently. The proxy process
itself does not use this per-BaseContainer configuration.

MXC also sets the standard proxy environment variables for libraries that use cooperative proxying. The OS permits
outbound traffic only to the configured loopback proxy address and port; direct or proxy-bypassing traffic is blocked.

The omitted `network` block uses the default-deny policy. With
`runtimeConfig.networkProxy`, an explicit block with `egress.default: "deny"`,
`ingress.default: "deny"`, and `ingress.hostLoopback: "deny"` is equivalent and
forms model 2. Proxy mode cannot contain direct egress allow or deny rules.

The proxy endpoint is runtime metadata, not shared network policy. MXC resolves
`allowedProxyPeer` when provided, authorizes outbound traffic only to that peer
and endpoint, and configures the per-container WinHTTP proxy. A host-process
proxy is also an outbound destination; it does not require
`ingress.hostLoopback: "allow"`.

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
- A schema 0.8 network request may use an AppContainer compatibility fallback
  only when WFP, firewall, and loopback rules preserve the exact egress and
  ingress policy. If a broad capability such as `privateNetworkClientServer`
  cannot be narrowed to the requested proxy endpoint and directions, MXC
  rejects the launch rather than widening access.

## 3. WFP is the enforcement primitive (both tiers)

**Admin requirement.** Adding WFP filters is admin-only. On Tier 1 the OS applies them in its own elevated context; on Tier 2 (Windows 23H2) MXC elevates on each launch to write the filters.

**Cleanup.** Filters will need to have a lifetime ≤ sandbox lifetime. In both tiers the filters will need to be cleaned up when there are no more processes running in the container.

AppContainer capabilities are coarse prerequisites, not the policy contract.
In particular, `privateNetworkClientServer` can enable both private-network
client and server behavior. MXC must use directional WFP and firewall
enforcement so `egress` still governs all outbound traffic and `ingress` still
governs all inbound traffic. A backend tier that cannot preserve those
directions is unsupported for that request.
