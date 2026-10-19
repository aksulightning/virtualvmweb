// Application-owned noVNC integration; the guest UUID selects a server-authorized socket.
import RFB from '/novnc/core/rfb.js';

const screen = document.querySelector('#console-screen');
const status = document.querySelector('#console-status');
const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
const rfb = new RFB(screen, `${protocol}//${location.host}/machines/${screen.dataset.domain}/ws`);
rfb.scaleViewport = true;
rfb.resizeSession = false;
rfb.background = '#101719';
rfb.addEventListener('connect', () => { status.textContent = 'Yhdistetty · näppäimistö ja hiiri käytössä'; });
rfb.addEventListener('disconnect', () => { status.textContent = 'Konsoliyhteys päättyi. Tarkista koneen tila ja kirjaudu tarvittaessa uudelleen.'; });
rfb.addEventListener('securityfailure', () => { status.textContent = 'Konsolin tunnistautuminen epäonnistui.'; });
document.querySelector('#send-cad').addEventListener('click', () => rfb.sendCtrlAltDel());
document.querySelector('#fullscreen').addEventListener('click', () => screen.requestFullscreen().catch(() => { status.textContent = 'Selain ei sallinut koko näytön tilaa.'; }));
window.addEventListener('pagehide', () => rfb.disconnect());

