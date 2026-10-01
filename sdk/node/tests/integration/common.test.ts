// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from 'node:assert';
import { describe, it } from 'node:test';
import os from 'os';
import {
  sdk,
  supportedVersions,
  debugSpawnOptions,
} from './test-helpers.js';

describe('Platform support', () => {
  it('should report platform support information', () => {
    const support = sdk.getPlatformSupport();
    assert.ok(typeof support.isSupported === 'boolean', 'isSupported should be a boolean');
    assert.ok(Array.isArray(support.availableMethods), 'availableMethods should be an array');
  });
});

const platformSupport = sdk.getPlatformSupport();

// The exact 0.6 contract predates Seatbelt, which is the native macOS backend.
const platformVersions = os.platform() === 'darwin'
  ? supportedVersions.filter((version) => version.compare('0.7.0-alpha') >= 0)
  : supportedVersions;

for (const schemaVersion of platformVersions) {
  const skipReason = !platformSupport.isSupported
    ? `Platform not supported: ${platformSupport.reason}`
    : undefined;

  describe(`One-shot smoke tests (schema ${schemaVersion})`, { skip: skipReason }, () => {
    const policy = {
      version: schemaVersion.raw,
      filesystem: {
        readwritePaths: [os.tmpdir()],
        readonlyPaths: [process.cwd()],
      },
      network: {
        allowOutbound: false,
      },
      ui: {
        allowWindows: false,
      },
      timeoutMs: 30000,
    };

    it('should run asynchronously from ContainerConfig', async () => {
      const config = sdk.createConfigFromPolicy(policy);
      config.process = config.process ?? { commandLine: '' };
      config.process.commandLine = 'cmd.exe /c echo test';
      config.containerId = `run-async-${schemaVersion}`;

      const result = await sdk.runAsync(config, debugSpawnOptions);
      assert.strictEqual(result.exitCode, 0);
      assert.ok(result.stdout.includes('test'));
    });

    it('should reject executor-only dry-run', async () => {
      const config = sdk.createConfigFromPolicy(policy);
      config.process!.commandLine = 'cmd.exe /c echo test';
      await assert.rejects(
        sdk.runAsync(config, { dryRun: true }),
        /does not support option 'dryRun'/,
      );
    });
  });
}
