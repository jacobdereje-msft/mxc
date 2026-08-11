## Examples

For a more comprehensive list of examples, look in the examples\ directory.

### Basic Hello World
```json
{
  "script": "python -c \"import sys; print('Hello from MXC!'); print(f'Python version: {sys.version}');\"",
  "processContainer": {
    "name": "CLI-HelloWorld"
  }
}
```

### Filesystem Access Control
```json
{
  "script": "python -c \"open('C:\\\\temp\\\\output.txt', 'w').write('test')\"",
  "processContainer": {
    "name": "CLI-Filesystem-Test"
  },
  "filesystem": {
    "readwritePaths": [
      "C:\\temp"
    ],
    "deniedPaths": [
      "C:\\Windows\\System32"
    ],
    "clearPolicyOnExit": true
  }
}
```

### ProcessContainer Networking

Starting in schema 0.8, ProcessContainer networking adds IP/CIDR, protocol, and
port rules. Schema 0.7.0 and earlier retain their existing `network.proxy`
shape and cooperative proxy environment-variable behavior. The enforced
AppContainer proxy-peer model is new in 0.8.

Ingress intentionally has no source, destination, or port rules. Its
`default` field controls general inbound traffic, while `hostLoopback`
separately controls host-loopback connectivity in either direction.

This direct-egress example permits only TCP/443 to one destination:

```jsonc
{
  "version": "0.8.0-dev",
  "containment": "processcontainer",
  "network": {
    "egress": {
      "default": "deny",
      "allow": [
        {
          "to": [ { "cidr": "140.82.112.0/20" } ],
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

For proxy-only egress, leave egress deny-default with no direct rules and
provide the loopback endpoint and one proxy identity outside the shared
policy:

```jsonc
{
  "version": "0.8.0-dev",
  "containment": "processcontainer",
  "network": {
    "egress": {
      "default": "deny"
    },
    "ingress": {
      "default": "deny",
      "hostLoopback": "deny"
    }
  },
  "runtimeConfig": { // Runtime data passed to MXC, not policy.
    // GA accepts only localhost, 127.0.0.1, or [::1] with a port.
    "networkProxy": "http://127.0.0.1:8080"
  },
  "processContainer": {
    "network": {
      // Package family name of the packaged AppContainer proxy.
      "allowedProxyPeer": "Contoso.AgentProxy_1234567890abc"
    }
  }
}
```

For a proxy that is not packaged as an AppContainer, omit
`allowedProxyPeer` and explicitly set `network.ingress.hostLoopback` to
`"allow"`. This opts into reaching a host loopback proxy rather than a
peer-isolated proxy:

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

The proxy must already be running. A packaged proxy needs
`privateNetworkClientServer`, `internetClient` when it connects externally,
and package-owned inbound firewall authorization. See
[Process Container Networking Configuration](process-container/networking.md)
for both proxy setups.