import assert from 'node:assert/strict'
import { test } from 'vitest'
import { buildPlan, cargoArtifact } from './build-local-runtime.mjs'

const host = 'aarch64-apple-darwin'

test('Tauri target and debug mode override host build defaults', () => {
  const plan = buildPlan([], { TAURI_ENV_ARCH: 'x86_64', TAURI_ENV_PLATFORM: 'darwin', TAURI_ENV_DEBUG: 'true' }, host)
  assert.deepEqual(plan.targets, ['x86_64-apple-darwin'])
  assert.equal(plan.profile, 'dev')
  assert.throws(() => buildPlan(['--target', host], { TAURI_ENV_ARCH: 'x86_64', TAURI_ENV_PLATFORM: 'darwin' }, host), /match the Tauri target/)
})

test('universal builds require both slices and explicit custom profiles survive', () => {
  const plan = buildPlan(['--target', 'universal-apple-darwin', '--profile', 'test-runtime'], {}, host)
  assert.deepEqual(plan.targets, [host, 'x86_64-apple-darwin'])
  assert.equal(plan.profile, 'test-runtime')
  assert.throws(() => buildPlan(['--profile', 'test-runtime'], { TAURI_ENV_DEBUG: 'false' }, host), /match the Tauri build profile/)
})

test('production cannot silently fall back to an ad-hoc signature', () => {
  assert.throws(() => buildPlan([], { TERMINALX_MAC_RELEASE: '1' }, host), /ad-hoc/)
  assert.equal(buildPlan([], { TERMINALX_MAC_RELEASE: '1', APPLE_SIGNING_IDENTITY: 'fixture-certificate' }, host).identity, 'fixture-certificate')
  assert.match(buildPlan(['--dev'], {}, host).identifier, /\.dev\./)
})

test('reject unsupported targets and malformed options rather than packaging the host binary', () => {
  assert.throws(() => buildPlan([], { TAURI_ENV_PLATFORM: 'linux', TAURI_ENV_ARCH: 'x86_64' }, host), /not supported/)
  assert.throws(() => buildPlan(['--dev'], { TAURI_ENV_DEBUG: 'false' }, host), /match the Tauri build profile/)
  assert.throws(() => buildPlan([], { TAURI_ENV_DEBUG: 'unexpected' }, host), /Unrecognized/)

  for (const args of [['--target', 'x86_64-unknown-linux-gnu'], ['--profile'], ['--unknown'], ['--profile', '../release']]) {
    assert.throws(() => buildPlan(args, {}, host))
  }
})

test('artifact selection uses the actual Cargo executable including custom target directories', () => {
  const artifact = { reason: 'compiler-artifact', target: { name: 'terminalx-serve' }, executable: '/private/custom/debug/terminalx-serve' }
  assert.equal(cargoArtifact([JSON.stringify({ reason: 'build-finished', success: true }), JSON.stringify(artifact)].join('\n')), artifact.executable)
  assert.throws(() => cargoArtifact(''), /exactly one/)
  assert.throws(() => cargoArtifact([JSON.stringify(artifact), JSON.stringify(artifact)].join('\n')), /exactly one/)
})
