// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { describe, it, before, after, afterEach } from 'node:test';
import assert from 'node:assert';
import { ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import fs from 'fs';
import os from 'os';
import path from 'path';
import {
  sdk,
  supportedVersions,
  sandboxSkipReason,
  createTempDir,
  withToolPaths,
  startTestProxy,
  pythonCommand,
  pythonSkipReason,
  runConfigForTest,
} from './test-helpers.js';

const proxySkipReason = sandboxSkipReason ??
  (process.env.MXC_ENABLE_PROCESSCONTAINER_PROXY_TESTS === '1'
    ? undefined
    : 'ProcessContainer proxy tests require an interactive/elevated WinHTTP proxy shim; set MXC_ENABLE_PROCESSCONTAINER_PROXY_TESTS=1 to run them');

for (const schemaVersion of supportedVersions) {
describe(`Windows Process Container (schema ${schemaVersion})`, {
  skip: os.platform() !== 'win32' ? 'Windows Process Container tests can only be ran on Windows' : undefined,
}, () => {
  let tempDir = '';

  afterEach(() => {
    if (tempDir && fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
      tempDir = '';
    }
  });

  for (const [name, operation] of [['run', sdk.run], ['runAsync', sdk.runAsync]] as const) {
    it(`public ${name} captures output and the workload exit code`, { skip: sandboxSkipReason }, async () => {
      const result = await operation({
        containment: { type: 'processcontainer' },
        command: 'cmd.exe /c "echo PUBLIC_RUN_OK & echo PUBLIC_RUN_ERROR 1>&2 & exit /b 7"',
        timeoutMs: 30000,
      });
      assert.strictEqual(result.exitCode, 7);
      assert.strictEqual(result.timedOut, false);
      assert.ok(result.stdout.includes('PUBLIC_RUN_OK'));
      assert.ok(result.stderr.includes('PUBLIC_RUN_ERROR'));
      assert.ok(Array.isArray(result.warnings));
    });
  }

  for (const [name, operation] of [['spawn', sdk.spawn], ['spawnAsync', sdk.spawnAsync]] as const) {
    it(`public ${name} returns an SDK process with live standard streams`, { skip: sandboxSkipReason }, async () => {
      const handle = await operation({
        containment: { type: 'processcontainer' },
        command: 'cmd.exe /c "echo PUBLIC_SPAWN_OK & echo PUBLIC_SPAWN_ERROR 1>&2 & exit /b 9"',
        timeoutMs: 30000,
      });
      try {
        const stdout = handle.standardOutput;
        const stderr = handle.standardError;
        assert.ok(stdout);
        assert.ok(stderr);
        const read = async (stream: NodeJS.ReadableStream): Promise<string> => {
          let text = '';
          for await (const chunk of stream) text += chunk.toString();
          return text;
        };
        const output = read(stdout);
        const error = read(stderr);
        const result = await handle.waitAsync();
        assert.strictEqual(result.exitCode, 9);
        assert.strictEqual(result.timedOut, false);
        assert.ok((await output).includes('PUBLIC_SPAWN_OK'));
        assert.ok((await error).includes('PUBLIC_SPAWN_ERROR'));
        assert.ok(Array.isArray(handle.warnings));
      } finally {
        handle.dispose();
      }
    });
  }

  it('should execute cmd.exe in process container', { skip: sandboxSkipReason }, async () => {
    const result = await sdk.runRequestForTest(
      'cmd.exe /c echo Container test successful',
      {},
      {},
      undefined,
      `test-1-${schemaVersion}`,
    );
    assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
    assert.ok(result.stdout.includes('Container test successful'));
  });

  it('should execute powershell 5.1 in process container', { skip: sandboxSkipReason }, async () => {
    const result = await sdk.runRequestForTest(
      "powershell.exe -NoProfile -Command Write-Output 'PowerShell test successful'",
      { ui: { disable: false } },
      {},
      undefined,
      `test-2-${schemaVersion}`,
    );
    assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
    assert.ok(result.stdout.includes('PowerShell test successful'));
  });

  it('should execute python in process container', { skip: sandboxSkipReason ?? pythonSkipReason }, async () => {
    const policy = withToolPaths({ ui: { disable: false } });
    const result = await sdk.runRequestForTest(
      `${pythonCommand} -c "print('Python test successful')"`,
      policy,
      {},
      undefined,
      `test-3-${schemaVersion}`,
    );
    assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
    assert.ok(result.stdout.includes('Python test successful'));
  });

  it('should allow writing to brokered readwrite path', { skip: sandboxSkipReason ?? pythonSkipReason }, async () => {
    tempDir = createTempDir();
    const testFile = path.join(tempDir, 'output.txt');
    const scriptFile = path.join(tempDir, 'write_test.py');
    fs.writeFileSync(scriptFile, `f = open(r'${testFile}', 'w')\nf.write('hello')\nf.close()\nprint('WRITE_OK')\n`);
    const policy = withToolPaths({
      ui: { disable: false },
      filesystem: { readwritePaths: [tempDir] },
    });
    const result = await sdk.runRequestForTest(
      `${pythonCommand} ${scriptFile}`,
      policy,
      {},
      tempDir,
      `test-4-${schemaVersion}`,
    );
    assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
    assert.ok(result.stdout.includes('WRITE_OK'));
    assert.ok(fs.existsSync(testFile), 'File should have been written to readwrite path');
  });

  it('should allow reading from brokered readonly path', { skip: sandboxSkipReason }, async () => {
    tempDir = createTempDir();
    fs.writeFileSync(path.join(tempDir, 'input.txt'), 'readonly test data');
    const inputFile = path.join(tempDir, 'input.txt');
    const policy = withToolPaths({
      filesystem: { readonlyPaths: [tempDir] },
    });
    const result = await sdk.runRequestForTest(
      `cmd.exe /c type ${inputFile}`,
      policy,
      {},
      tempDir,
      `test-5-${schemaVersion}`,
    );
    assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
    assert.ok(result.stdout.includes('readonly test data'));
  });

  it('should launch basic process container with valid version', { skip: sandboxSkipReason }, async () => {
    const result = await sdk.runRequestForTest(
      'cmd.exe /c echo version ok',
      {},
      {},
      undefined,
      `test-ver-${schemaVersion}`,
    );
    assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
    assert.ok(result.stdout.includes('version ok'));
  });

  describe('proxy end-to-end', { skip: proxySkipReason }, () => {
    let proxyProcess: ChildProcess | null = null;
    let originalMaxListeners: number;

    // Proxy tests can accumulate socket listeners when connections hang (e.g. BaseContainer proxy issues).
    // Raise the cap to avoid spurious MaxListenersExceededWarning noise in test output.
    before(() => {
      originalMaxListeners = EventEmitter.defaultMaxListeners;
      EventEmitter.defaultMaxListeners = 30;
    });
    after(() => {
      EventEmitter.defaultMaxListeners = originalMaxListeners;
    });

    afterEach(() => {
      if (proxyProcess) {
        proxyProcess.kill();
        proxyProcess = null;
      }
    });

    it('should route traffic through external proxy', async () => {
      tempDir = createTempDir('mxc-proxy-test');
      const { port, proxyProcess: proc } = startTestProxy(tempDir);
      proxyProcess = proc;

      const config = sdk.createConfigForTest(
        withToolPaths({ ui: { disable: false } }),
        'processcontainer',
        `proxy-ext-${schemaVersion}`,
      );
      config.processContainer!.capabilities = ['internetClient'];
      config.network = {
        egress: { default: 'deny' },
        ingress: { default: 'deny', hostLoopback: 'deny' },
      };
      config.runtimeConfig = {
        ...(config.runtimeConfig ?? {}),
        networkProxy: `http://127.0.0.1:${port}`,
      };
      const script =
        `powershell.exe -NoProfile -Command "` +
        `$h = New-Object -ComObject WinHttp.WinHttpRequest.5.1; ` +
        `$h.Open('GET','https://api.github.com/zen',$false); ` +
        `$h.Send(); ` +
        `Write-Output ('PROXY_RESPONSE: ' + $h.ResponseText)"`;
      config.process!.commandLine = script;
      const result = await runConfigForTest(config, { experimental: true });

      assert.strictEqual(result.exitCode, 0, `[${schemaVersion}] Expected exit 0: ${result.stderr}`);
      assert.ok(result.stdout.includes('PROXY_RESPONSE:'));
      assert.ok(result.stdout.includes('Proxy policy active'));
    });
  });
});
}
