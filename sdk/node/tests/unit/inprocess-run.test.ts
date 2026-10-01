// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from 'node:assert';
import { afterEach, describe, it } from 'node:test';
import { MxcError } from '../../src/errors.js';
import {
  run,
  runAsync,
  spawn,
  spawnAsync,
} from '../../src/sandbox.js';
import {
  _setBindingRunAsyncImplementation,
  _setBindingRunImplementation,
} from '../../src/bindings/run.js';
import { _setBindingSandboxProcessFactories } from '../../src/bindings/streaming.js';
import type { RequestSpec } from '../../src/bindings/request.js';
import type { MxcSandboxProcess } from '../../src/sandbox-process.js';
import type { ContainerConfig } from '../../src/types.js';

afterEach(() => {
  _setBindingRunImplementation();
  _setBindingRunAsyncImplementation();
  _setBindingSandboxProcessFactories();
});

function config(): ContainerConfig {
  return {
    version: '0.9.0-alpha',
    containerId: 'sample',
    process: {
      commandLine: 'echo hello',
      cwd: 'C:\\work',
      env: ['FROM_CONFIG=value'],
      inheritDefaultEnv: true,
    },
  };
}

const bindingResult = {
  stdout: 'out',
  stderr: 'err',
  exitCode: 7,
  timedOut: false,
  outputMetadata: { captureDenials: { outputPath: 'denials.json' } },
  warnings: ['policy was relaxed'],
};

describe('in-process one-shot APIs', () => {
  it('runs synchronously from ContainerConfig', () => {
    let bindingRequest: RequestSpec | undefined;
    _setBindingRunImplementation((request) => {
      bindingRequest = request;
      return bindingResult;
    });

    const result = run(config(), { experimental: true });

    assert.deepStrictEqual(result, bindingResult);
    assert.strictEqual(bindingRequest?.policy.version, '0.9.0-alpha');
    assert.strictEqual(bindingRequest?.command, 'echo hello');
    assert.strictEqual(bindingRequest?.containerName, 'sample');
    assert.strictEqual(bindingRequest?.workingDirectory, 'C:\\work');
    assert.deepStrictEqual(bindingRequest?.environment, { FROM_CONFIG: 'value' });
    assert.strictEqual(bindingRequest?.inheritDefaultEnv, true);
    assert.strictEqual(bindingRequest?.experimental, true);
  });

  it('runs asynchronously from ContainerConfig', async () => {
    let bindingRequest: RequestSpec | undefined;
    _setBindingRunAsyncImplementation(async (request) => {
      bindingRequest = request;
      return { ...bindingResult, timedOut: true };
    });

    const result = await runAsync(config());

    assert.strictEqual(result.timedOut, true);
    assert.strictEqual(result.exitCode, 7);
    assert.strictEqual(bindingRequest?.command, 'echo hello');
  });

  it('spawns synchronously and asynchronously through native bindings', async () => {
    const syncProcess = { kind: 'sync' } as unknown as MxcSandboxProcess;
    const asyncProcess = { kind: 'async' } as unknown as MxcSandboxProcess;
    let syncRequest: RequestSpec | undefined;
    let asyncRequest: RequestSpec | undefined;
    _setBindingSandboxProcessFactories(
      (request) => {
        syncRequest = request;
        return syncProcess;
      },
      async (request) => {
        asyncRequest = request;
        return asyncProcess;
      },
    );

    assert.strictEqual(spawn(config()), syncProcess);
    assert.strictEqual(await spawnAsync(config()), asyncProcess);
    assert.strictEqual(syncRequest?.command, 'echo hello');
    assert.strictEqual(asyncRequest?.command, 'echo hello');
  });

  it('rejects executor-only options instead of falling back', async () => {
    for (const options of [
      { dryRun: true },
      { skipPlatformCheck: true },
      { executablePath: 'wxc-exec.exe' },
      { signal: new AbortController().signal },
      { debug: true },
    ]) {
      assert.throws(
        () => run(config(), options),
        /one-shot execution does not support option/,
      );
      await assert.rejects(
        runAsync(config(), options),
        /one-shot execution does not support option/,
      );
    }
  });

  it('preserves typed native errors', async () => {
    const error = new MxcError('unsupported_containment', 'LXC is executor-only');
    _setBindingRunImplementation(() => {
      throw error;
    });
    _setBindingRunAsyncImplementation(async () => {
      throw error;
    });

    assert.throws(() => run(config()), (value) => value === error);
    await assert.rejects(runAsync(config()), (value) => value === error);
  });

  it('wraps native invocation failures as backend errors', async () => {
    _setBindingRunImplementation(() => {
      throw new Error('native invocation failed');
    });
    _setBindingRunAsyncImplementation(async () => {
      throw new Error('native invocation failed');
    });

    assert.throws(
      () => run(config()),
      (error: unknown) =>
        error instanceof MxcError
        && error.code === 'backend_error'
        && error.message === 'native invocation failed',
    );
    await assert.rejects(
      runAsync(config()),
      (error: unknown) =>
        error instanceof MxcError
        && error.code === 'backend_error'
        && error.message === 'native invocation failed',
    );
  });

  it('requires process.commandLine', () => {
    assert.throws(
      () => run({ version: '0.9.0-alpha' }),
      (error: unknown) =>
        error instanceof MxcError
        && error.code === 'malformed_request'
        && error.message === 'process.commandLine is required on ContainerConfig.',
    );
  });
});
