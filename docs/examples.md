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

ProcessContainer networking differs between schemas 0.7 and 0.8. See the
[schema 0.7](process-container/examples/0.7.0-schema.md) and
[schema 0.8](process-container/examples/0.8.0-schema.md) ProcessContainer
examples. Enforcement details are in
[Process Container Networking Configuration](process-container/networking.md).