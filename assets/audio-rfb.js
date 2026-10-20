// Minimal shared RFB 3.8 client for QEMU playback only; never requests pixels or sends input.
export class AudioRfb {
  /** Inject transport and PCM consumers so fragmented wire messages can be tested. */
  constructor(send, pcm, ready) {
    this.send = send;
    this.pcm = pcm;
    this.ready = ready;
    this.pending = new Uint8Array(0);
    this.state = 'version';
  }

  /** Consume bounded, possibly fragmented WebSocket payloads without assuming frame alignment. */
  feed(bytes) {
    if (this.pending.length + bytes.length > 1048576) throw Error('Liian suuri äänipaketti.');
    const joined = new Uint8Array(this.pending.length + bytes.length);
    joined.set(this.pending);
    joined.set(bytes, this.pending.length);
    this.pending = joined;
    while (this.parse()) { /* Drain complete protocol messages. */ }
  }

  /** Parse one complete message; leave incomplete input queued for the next frame. */
  parse() {
    const b = this.pending;
    const d = new DataView(b.buffer, b.byteOffset, b.byteLength);
    let size = 0;
    if (this.state === 'version') {
      if (b.length < 12) return false;
      if (new TextDecoder().decode(b.subarray(0, 12)) !== 'RFB 003.008\n') throw Error('RFB 3.8 vaaditaan.');
      this.send(new TextEncoder().encode('RFB 003.008\n'));
      size = 12;
      this.state = 'security';
    } else if (this.state === 'security') {
      if (!b.length) return false;
      if (!b[0]) throw Error('Ääniyhteys hylättiin.');
      size = 1 + b[0];
      if (b.length < size) return false;
      if (!b.subarray(1, size).includes(1)) throw Error('Libvirt-ääniyhteys ei ole käytettävissä.');
      this.send(new Uint8Array([1]));
      this.state = 'result';
    } else if (this.state === 'result') {
      if (b.length < 4) return false;
      if (d.getUint32(0)) throw Error('Ääniyhteyden tunnistus epäonnistui.');
      this.send(new Uint8Array([1])); // Shared connection preserves the visible console.
      size = 4;
      this.state = 'init';
    } else if (this.state === 'init') {
      if (b.length < 24) return false;
      size = 24 + d.getUint32(20);
      if (size > 65536) throw Error('Virheellinen RFB-palvelimen nimi.');
      if (b.length < size) return false;
      this.send(new Uint8Array([2, 0, 0, 1, 255, 255, 254, 253])); // Encoding -259.
      this.state = 'ack';
    } else {
      if (!b.length) return false;
      if (b[0] === 0 && this.state === 'ack') {
        if (b.length < 16) return false;
        if (d.getUint16(2) !== 1 || d.getInt32(12) !== -259) throw Error('QEMU-äänilaajennus puuttuu.');
        // Signed 16-bit little-endian stereo, 44100 Hz; QEMU defaults to LE.
        this.send(new Uint8Array([255, 1, 0, 2, 3, 2, 0, 0, 172, 68]));
        this.send(new Uint8Array([255, 1, 0, 0]));
        this.state = 'audio';
        this.ready();
        size = 16;
      } else if (b[0] === 255 && this.state === 'audio') {
        if (b.length < 4) return false;
        const kind = d.getUint16(2);
        if (b[1] !== 1 || kind > 2) throw Error('Tuntematon ääniviesti.');
        size = 4;
        if (kind === 2) {
          if (b.length < 8) return false;
          const count = d.getUint32(4);
          if (count > 176400 || count % 4) throw Error('Virheellinen PCM-paketti.');
          size = 8 + count;
          if (b.length < size) return false;
          if (count) this.pcm(b.subarray(8, size));
        }
      } else if (b[0] === 2) {
        size = 1; // Ignore the console bell, which is not guest PCM audio.
      } else if (b[0] === 3) {
        if (b.length < 8) return false;
        size = 8 + d.getUint32(4); // Discard unsolicited clipboard text.
        if (size > 65536) throw Error('Liian suuri RFB-viesti.');
        if (b.length < size) return false;
      } else {
        throw Error('Odottamaton RFB-ääniviesti.');
      }
    }
    this.pending = b.slice(size);
    return true;
  }
}

/** Convert QEMU interleaved PCM to Web Audio channels; reject partial stereo samples. */
export function decodePcm(bytes) {
  if (bytes.length % 4) throw Error('Virheellinen PCM-paketti.');
  const frames = bytes.length / 4;
  const channels = [new Float32Array(frames), new Float32Array(frames)];
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < frames; i++) {
    channels[0][i] = view.getInt16(i * 4, true) / 32768;
    channels[1][i] = view.getInt16(i * 4 + 2, true) / 32768;
  }
  return channels;
}
