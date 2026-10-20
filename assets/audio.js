// User-initiated VM playback over the existing authenticated, origin-checked VNC bridge.
import { AudioRfb, decodePcm } from './audio-rfb.js';

/** Bind one console's playback controls, bounded scheduling, and navigation cleanup. */
export function initAudio(url) {
  const button = document.querySelector('#audio-toggle');
  const status = document.querySelector('#audio-status');
  const volume = document.querySelector('#audio-volume');
  let active = null;

  /** Close transport and audio graph, including already scheduled sounds. */
  function stop(message = 'Ääni pois päältä.') {
    const old = active;
    active = null;
    if (old) {
      clearTimeout(old.timer);
      old.ws?.close();
      void old.context.close().catch(() => {});
    }
    button.textContent = 'Ääni päälle';
    button.setAttribute('aria-pressed', 'false');
    status.textContent = message;
  }

  /** Decode and schedule at most half a second of sound; discard backlog after tab suspension. */
  function play(state, bytes) {
    if (active !== state || state.context.state !== 'running') return;
    const now = state.context.currentTime;
    if (state.next > now + 0.5) return;
    const channels = decodePcm(bytes);
    const buffer = state.context.createBuffer(2, channels[0].length, 44100);
    channels.forEach((channel, index) => buffer.copyToChannel(channel, index));
    const source = state.context.createBufferSource();
    source.buffer = buffer;
    source.connect(state.gain);
    source.onended = () => source.disconnect();
    state.next = Math.max(state.next, now + 0.04);
    source.start(state.next);
    state.next += buffer.duration;
  }

  /** Resume Web Audio inside a click gesture, then negotiate a separate shared RFB connection. */
  async function start() {
    if (active) { stop(); return; }
    let state;
    try {
      const context = new AudioContext({ latencyHint: 'interactive' });
      state = { context, gain: context.createGain(), next: 0 };
      active = state;
      state.gain.gain.value = Number(volume.value);
      state.gain.connect(context.destination);
      button.textContent = 'Mykistä';
      button.setAttribute('aria-pressed', 'true');
      status.textContent = 'Yhdistetään ääneen…';
      await context.resume();
      if (active !== state) return;
      const ws = new WebSocket(url);
      state.ws = ws;
      ws.binaryType = 'arraybuffer';
      state.timer = setTimeout(() => {
        if (active === state) stop('Ääni ei vastaa. Ota selaimen äänilaite käyttöön sammutetun koneen asetuksista.');
      }, 10000);
      const protocol = new AudioRfb(bytes => ws.send(bytes), bytes => play(state, bytes), () => {
        clearTimeout(state.timer);
        status.textContent = 'Ääni käytössä. Tarkista tarvittaessa vieraan äänenvoimakkuus.';
      });
      ws.onmessage = event => {
        if (active !== state) return;
        try {
          if (!(event.data instanceof ArrayBuffer)) throw Error('Virheellinen äänikehys.');
          protocol.feed(new Uint8Array(event.data));
        } catch (error) { stop(error.message); }
      };
      ws.onerror = () => { if (active === state) stop('Ääniyhteys epäonnistui. Tarkista kirjautuminen ja vapaiden konsoliyhteyksien määrä.'); };
      ws.onclose = () => { if (active === state) stop('Ääniyhteys suljettiin.'); };
    } catch (error) {
      if (!state || active === state) stop(`Äänen käynnistys epäonnistui: ${error.message}`);
    }
  }
  button.addEventListener('click', start);
  volume.addEventListener('input', () => {
    if (active) active.gain.gain.value = Number(volume.value);
  });
  window.addEventListener('pagehide', () => stop());
  return stop;
}
