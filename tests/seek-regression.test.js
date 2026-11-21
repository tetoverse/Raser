'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function createDeferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function flushMicrotasks() {
  return new Promise((resolve) => setImmediate(resolve));
}

function loadBridgeHarness() {
  const invokeCalls = [];
  let invokeHandler = () => Promise.resolve();
  const intervals = [];

  class FakeAudio extends EventTarget {
    constructor() {
      super();
      this.attributes = new Map();
    }
    removeAttribute(name) {
      this.attributes.delete(name);
    }
  }

  const audio = new FakeAudio();
  const docListeners = new Map();
  const fakeDocument = {
    hidden: false,
    getElementById: (id) => (id === 'native-audio' ? audio : null),
    querySelector: (sel) => (sel === 'audio' ? audio : null),
    addEventListener: (ev, fn) => {
      if (!docListeners.has(ev)) docListeners.set(ev, []);
      docListeners.get(ev).push(fn);
    },
  };

  const windowListeners = new Map();
  const localStorageMap = new Map();
  const fakeWindow = {
    __TAURI__: {
      core: {
        invoke: (cmd, args) => {
          invokeCalls.push({ cmd, args });
          return invokeHandler(cmd, args);
        },
      },
      event: {
        listen: () => Promise.resolve(() => {}),
      },
    },
    addEventListener: (ev, fn) => {
      if (!windowListeners.has(ev)) windowListeners.set(ev, []);
      windowListeners.get(ev).push(fn);
    },
    dispatchEvent: () => true,
  };
  fakeWindow.window = fakeWindow;

  const context = {
    window: fakeWindow,
    document: fakeDocument,
    Event,
    CustomEvent: class CustomEvent extends Event {
      constructor(type, opts = {}) {
        super(type);
        this.detail = opts.detail;
      }
    },
    localStorage: {
      length: 0,
      key: () => null,
      getItem: (k) => localStorageMap.get(k) ?? null,
      setItem: (k, v) => localStorageMap.set(k, String(v)),
    },
    performance: { now: () => Date.now() },
    URL,
    Uint8Array,
    console,
    setTimeout,
    clearTimeout,
    setInterval: (fn, ms) => {
      const id = intervals.length + 1;
      intervals.push({ id, fn, ms });
      return id;
    },
    clearInterval: () => {},
  };

  const bridgePath = path.resolve(__dirname, '../ui/tauri-bridge.js');
  const code = fs.readFileSync(bridgePath, 'utf8');
  vm.runInNewContext(code, context, { filename: bridgePath });

  return {
    audio,
    window: fakeWindow,
    invokeCalls,
    setInvokeHandler: (fn) => {
      invokeHandler = fn;
    },
    triggerPoll: () => {
      assert.ok(intervals.length > 0, 'polling interval should be registered');
      intervals[0].fn();
    },
  };
}

test('rapid seeks coalesce to single-flight latest-wins and defer seeked until settled', async () => {
  const h = loadBridgeHarness();
  const seekQueue = [];
  let loadResolve;

  h.setInvokeHandler((cmd, args) => {
    if (cmd === 'rust_audio_load') {
      return new Promise((res) => {
        loadResolve = () => res({ position: 0, duration: 200, playing: true, loading: false, ended: false });
      });
    }
    if (cmd === 'rust_audio_seek') {
      const d = createDeferred();
      seekQueue.push({ position: args.position, deferred: d });
      return d.promise;
    }
    return Promise.resolve();
  });

  h.audio.src = 'https://example.com/track.mp3';
  loadResolve();
  await flushMicrotasks();

  let seekingCount = 0;
  let seekedCount = 0;
  h.audio.addEventListener('seeking', () => seekingCount++);
  h.audio.addEventListener('seeked', () => seekedCount++);

  // Simulate rapid slider dragging across 8 values while the first IPC is still in flight
  for (const pos of [20, 35, 50, 70, 95, 110, 125, 140]) {
    h.audio.currentTime = pos;
  }

  assert.equal(h.audio.seeking, true);
  assert.equal(seekingCount, 1);
  assert.equal(seekedCount, 0, 'seeked must not fire before backend seek finishes');
  assert.equal(seekQueue.length, 1, 'only 1 seek IPC should be in flight');
  assert.equal(seekQueue[0].position, 20);

  // Finish the first seek; bridge should now dispatch ONLY ONE more seek for the latest target (140)
  seekQueue[0].deferred.resolve({ position: 20.0, duration: 200, playing: true, loading: false, ended: false });
  await flushMicrotasks();

  assert.equal(seekQueue.length, 2, 'queued intermediate targets must coalesce to the latest target');
  assert.equal(seekQueue[1].position, 140);
  assert.equal(h.audio.seeking, true);
  assert.equal(seekedCount, 0);

  // Finish the latest seek; position should reflect backend result and seeked should fire
  seekQueue[1].deferred.resolve({ position: 139.92, duration: 200, playing: true, loading: false, ended: false });
  await flushMicrotasks();

  assert.equal(h.audio.seeking, false);
  assert.equal(seekedCount, 1);
  assert.equal(h.audio.currentTime, 139.92);
});

test('stale state polls in flight during seek do not overwrite completed seek position', async () => {
  const h = loadBridgeHarness();
  const seekDeferred = createDeferred();
  const pollDeferred = createDeferred();
  let pollCount = 0;

  h.setInvokeHandler((cmd) => {
    if (cmd === 'rust_audio_load') {
      return Promise.resolve({ position: 0, duration: 180, playing: true, loading: false, ended: false });
    }
    if (cmd === 'rust_audio_seek') {
      return seekDeferred.promise;
    }
    if (cmd === 'rust_audio_get_state') {
      pollCount++;
      if (pollCount === 1) return pollDeferred.promise;
      return Promise.resolve({ position: 105.4, duration: 180, playing: true, loading: false, ended: false });
    }
    return Promise.resolve();
  });

  h.audio.src = 'file:///song.mp3';
  await flushMicrotasks();
  await h.audio.play();
  await flushMicrotasks();

  // Start a state poll BEFORE seek completes (simulating an overlapping 55ms poll)
  h.triggerPoll();
  assert.equal(pollCount, 1);

  h.audio.currentTime = 105;
  assert.equal(h.audio.seeking, true);

  // Complete the seek
  seekDeferred.resolve({ position: 105.0, duration: 180, playing: true, loading: false, ended: false });
  await flushMicrotasks();
  assert.equal(h.audio.currentTime, 105.0);
  assert.equal(h.audio.seeking, false);

  // Now the earlier stale poll resolves with pre-seek position 4.2s; it must be ignored
  pollDeferred.resolve({ position: 4.2, duration: 180, playing: true, loading: false, ended: false });
  await flushMicrotasks();
  assert.equal(h.audio.currentTime, 105.0, 'stale pre-seek poll must not overwrite seeked position');

  // Subsequent fresh poll updates normally
  h.triggerPoll();
  await flushMicrotasks();
  assert.equal(h.audio.currentTime, 105.4);
});

test('failed seek restores backend position instead of leaving stale target or resetting to 0', async () => {
  const h = loadBridgeHarness();
  const seekDeferred = createDeferred();

  h.setInvokeHandler((cmd) => {
    if (cmd === 'rust_audio_load') {
      return Promise.resolve({ position: 0, duration: 180, playing: true, loading: false, ended: false });
    }
    if (cmd === 'rust_audio_seek') {
      return seekDeferred.promise;
    }
    if (cmd === 'rust_audio_get_state') {
      return Promise.resolve({ position: 42.5, duration: 180, playing: true, loading: false, ended: false });
    }
    return Promise.resolve();
  });

  h.audio.src = 'https://example.com/stream.ogg';
  await flushMicrotasks();

  let seekedCount = 0;
  h.audio.addEventListener('seeked', () => seekedCount++);

  h.audio.currentTime = 120;
  assert.equal(h.audio.seeking, true);

  seekDeferred.reject(new Error('Stream seek failed'));
  await flushMicrotasks();
  await flushMicrotasks();

  assert.equal(h.audio.seeking, false);
  assert.equal(seekedCount, 1);
  assert.equal(h.audio.currentTime, 42.5, 'failed seek should sync back to backend position');
});

test('seek requested while loading waits for load completion before seeking', async () => {
  const h = loadBridgeHarness();
  const loadDeferred = createDeferred();
  const seekCalls = [];

  h.setInvokeHandler((cmd, args) => {
    if (cmd === 'rust_audio_load') return loadDeferred.promise;
    if (cmd === 'rust_audio_seek') {
      seekCalls.push(args.position);
      return Promise.resolve({ position: args.position, duration: 120, playing: false, loading: false, ended: false });
    }
    return Promise.resolve();
  });

  h.audio.src = 'https://example.com/track2.mp3';
  h.audio.currentTime = 30;
  assert.equal(seekCalls.length, 0, 'seek should not run while audio is still loading');
  assert.equal(h.audio.seeking, true);

  loadDeferred.resolve({ position: 0, duration: 120, playing: false, loading: false, ended: false });
  await flushMicrotasks();
  await flushMicrotasks();

  assert.deepEqual(seekCalls, [30]);
  assert.equal(h.audio.seeking, false);
  assert.equal(h.audio.currentTime, 30);
});
