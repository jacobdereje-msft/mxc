# ProcessContainer examples

These examples separate the schema 0.7 compatibility surface from the new
schema 0.8 network model. See
[Process Container Networking Configuration](../networking.md) for Windows
enforcement details and
[MXC Network Configuration](../../sandbox-policy/v2/networking.md) for the
shared policy semantics.

## Schema 0.7 proxy compatibility

Schema 0.7 and earlier retain the existing `network.proxy` shape. The
ProcessContainer compatibility path injects cooperative `HTTP_PROXY` and
`HTTPS_PROXY` variables for clients that honor them; it does not use the 0.8
proxy-peer contract.

```jsonc
{
  "version": "0.7.0",
  "containment": "processcontainer",
  "network": {
    "proxy": {
      "localhost": 8080
    }
  }
}
```

## Schema 0.8 direct egress

This policy permits only TCP/443 to the selected CIDR, except for one excluded
address. Egress, LAN/private-network inbound, and host loopback all default to
deny.

```jsonc
{
  "version": "0.8.0-dev",
  "containment": "processcontainer",
  "network": {
    "egress": {
      "default": "deny",
      "allow": [
        {
          "to": [
            {
              "cidr": "140.82.112.0/20",
              "except": [ "140.82.112.2/32" ]
            }
          ],
          "ports": [ { "protocol": "tcp", "port": 443 } ]
        }
      ],
      "deny": []
    },
    "ingress": {
      "default": "deny",
      "hostLoopback": "deny"
    }
  }
}
```

## Schema 0.8 contained proxy

For proxy-only egress, omit the `network` block to use its deny defaults.
Provide the loopback endpoint as runtime data and identify exactly one
contained proxy. `allowedProxyPeer` accepts either a packaged proxy's package
family name or an unpackaged proxy's AppContainer profile name.

```jsonc
{
  "version": "0.8.0-dev",
  "containment": "processcontainer",
  "runtimeConfig": {
    // GA accepts HTTP(S) loopback URLs with an explicit port.
    "networkProxy": "http://127.0.0.1:8080"
  },
  "processContainer": {
    "network": {
      "allowedProxyPeer": "agent-proxy"
    }
  }
}
```

Valid endpoint forms are:

- `http://localhost:<port>` or `https://localhost:<port>`
- `http://127.0.0.1:<port>` or `https://127.0.0.1:<port>`
- `http://[::1]:<port>` or `https://[::1]:<port>`

An explicit `network` block with `egress.default: "deny"`,
`ingress.default: "deny"`, and `ingress.hostLoopback: "deny"` is equivalent.
Proxy mode cannot contain direct egress allow or deny rules.

The proxy must already be running. Both contained security environments need
`privateNetworkClientServer`; the proxy also needs `internetClient` when it
connects externally and inbound firewall authorization for its executable.

## Schema 0.8 host-process proxy

If the proxy does not run in a packaged or unpackaged AppContainer, omit
`allowedProxyPeer` and explicitly allow the host-loopback path. This is a
deliberate relaxation from the identity-scoped contained-proxy model.

```jsonc
{
  "version": "0.8.0-dev",
  "containment": "processcontainer",
  "network": {
    "ingress": {
      "hostLoopback": "allow"
    }
  },
  "runtimeConfig": {
    "networkProxy": "http://127.0.0.1:8080"
  }
}
```

The omitted egress and ingress defaults remain deny. Direct internet egress
therefore stays blocked.
