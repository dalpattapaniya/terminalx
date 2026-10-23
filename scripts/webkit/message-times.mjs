#!/usr/bin/env node
// Synthetic messages through the real shared renderer; no app or agent launch.
import assert from 'node:assert/strict';
import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer, transformWithEsbuild } from 'vite';
import { launch } from './harness.mjs';
const entry = `
import React from 'react';
import { createRoot } from 'react-dom/client';
import { TurnBlock } from '/src/components/chat/TurnBlock.tsx';
import { buildTranscript } from '/packages/portable/src/transcript.ts';
import '/src/styles/app.css';
const events = [
  ['2026-10-22T23:59:00Z', {type:'user_message', text:'Synthetic prompt before midnight.'}],
  ['2026-10-23T00:01:00Z', {type:'assistant_text', text:'Synthetic reply after midnight.'}],
  ['2026-10-23T00:02:00Z', {type:'user_message', text:'Synthetic queued follow-up.', queued:true}],
  ['', {type:'assistant_text', text:'Synthetic historical reply with no reliable time.'}],
].map(([ts,payload],seq) => ({id:String(seq),seq,ts,messageTs:ts,payload,sessionId:'synthetic',tabId:'tab',harness:'claude'}));
events.push({id:'legacy',seq:4,ts:'2026-10-23T12:00:00Z',payload:{type:'assistant_text',text:'Synthetic legacy reply with an unverified import time.'},sessionId:'synthetic',tabId:'tab',harness:'claude'});
const turn = buildTranscript(events, true).turns[0];
const root = createRoot(document.getElementById('root'));
window.draw = (cloud = false, text = 'Synthetic streaming reply') => root.render(<main style={{padding:16}}><TurnBlock turn={turn} sessionId='synthetic' cwd={cloud ? undefined : '/synthetic/repo'} working stream={[{ref:{messageId:'stream',index:0},kind:'text',text,partialJson:'',done:false,ts:'2026-10-23T00:03:00Z'}]}/></main>);
window.draw();
`;
const server = await createServer({ server: { port: 0, host: '127.0.0.1' }, plugins: [{
  name: 'message-times-regression',
  resolveId(id) { if (id === '/message-times-test.tsx') return '\0message-times-test.tsx'; },
  async load(id) { if (id === '\0message-times-test.tsx') return (await transformWithEsbuild(entry, 'message-times-test.tsx', { loader: 'tsx', jsx: 'automatic' })).code; },
  configureServer(server) { server.middlewares.use('/timestamp-layout', async (_req, res) => {
    res.setHeader('Content-Type', 'text/html');
    res.end(await server.transformIndexHtml('/timestamp-layout', '<div id="root"></div><script type="module" src="/message-times-test.tsx"></script>'));
  }); },
}] });
const artifacts = await mkdtemp(join(tmpdir(), 'terminalx-600-'));
let browser;
try {
  await server.listen();
  browser = await launch();
  for (const settings of [
    { locale: 'en-US', timezoneId: 'UTC', viewport: { width: 800, height: 800 } },
    { locale: 'en-GB', timezoneId: 'Asia/Dubai', viewport: { width: 320, height: 850 }, hasTouch: true },
  ]) {
    const page = await browser.newPage(settings);
    const errors = [];
    page.on('pageerror', error => { errors.push(error.message); console.error(error.message); });
    await page.addInitScript(await readFile(new URL('./tauri-stub.js', import.meta.url), 'utf8'));
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/timestamp-layout`);
    await page.locator('time').nth(3).waitFor();
    assert.equal(await page.locator('time').count(), 4, 'one time per verified message, none for undated or legacy imported history');
    const times = await page.locator('time').allTextContents();
    await page.evaluate(() => window.draw(true, 'Synthetic streaming reply continues.'));
    assert.deepEqual(await page.locator('time').allTextContents(), times, 'cloud/stream updates preserve times');
    const summary = page.locator('summary').first();
    await summary.focus();
    await page.keyboard.press('Enter');
    assert.equal(await page.locator('details[open]').count(), 1, 'keyboard reveals full date');
    assert.match(await summary.getAttribute('aria-label'), settings.timezoneId === 'UTC' ? /October 22|22 October/ : /October 23|23 October/);
    await page.keyboard.press('Space');
    assert.equal(await page.locator('details[open]').count(), 0);
    if (settings.hasTouch) await summary.tap(); else await summary.click();
    assert.equal(await page.locator('details[open]').count(), 1, 'tap/click reveals full date');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no horizontal overflow');
    await page.screenshot({ path: join(artifacts, `${settings.locale}.png`), fullPage: true });
    await page.reload();
    await page.locator('time').nth(3).waitFor();
    assert.deepEqual(await page.locator('time').allTextContents(), times, 'reload keeps persisted times');
    assert.deepEqual(errors, []);
    await page.close();
  }
  console.log(`Message timestamp keyboard/touch, narrow layout, locale/timezone, streaming and reload checks passed. Screenshots: ${artifacts}`);
} finally {
  await browser?.close();
  await server.close();
}
