// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from 'node:assert';
import { describe, it } from 'node:test';
import { _errorCodeForNativeStatus } from '../../src/bindings/run.js';
import { nativeStatusError } from '../../src/bindings/native-error.js';

describe('native run binding', () => {
  it('owns generic structured details independently of native storage', () => {
    const payload = { diagnostic: { value: '9007199254740993', phases: ['prepare', 'launch'] } };
    const storage = Buffer.from(`${JSON.stringify(payload)}\0`);
    const error = nativeStatusError(12, {
      message: Buffer.from('generic failure\0'),
      detailsJson: storage,
    });
    storage.fill(0);
    assert.deepStrictEqual(error.details, { ...payload, ffiStatus: 12 });
    assert.strictEqual(error.code, 'backend_error');
    assert.strictEqual(error.message, 'generic failure');
  });

  it('maps SDK status codes', () => {
    assert.strictEqual(_errorCodeForNativeStatus(1), 'malformed_request');
    assert.strictEqual(_errorCodeForNativeStatus(2), 'unsupported_containment');
    assert.strictEqual(_errorCodeForNativeStatus(4), 'backend_unavailable');
    assert.strictEqual(_errorCodeForNativeStatus(11), 'policy_validation');
    assert.strictEqual(_errorCodeForNativeStatus(12), 'backend_error');
  });

  it('maps FFI contract failures without inventing new public codes', () => {
    assert.strictEqual(_errorCodeForNativeStatus(100), 'malformed_request');
    assert.strictEqual(_errorCodeForNativeStatus(101), 'malformed_request');
    assert.strictEqual(_errorCodeForNativeStatus(102), 'backend_error');
    assert.strictEqual(_errorCodeForNativeStatus(103), 'backend_error');
    assert.strictEqual(_errorCodeForNativeStatus(999), 'backend_error');
  });
});
