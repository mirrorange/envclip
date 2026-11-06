// Seal a drop exactly like the web UI does and print `url key`.
// usage: node test/send.mjs <server> NAME=value...
import * as ec from "../public/crypto.js";

const [server, ...pairs] = process.argv.slice(2);
const env = pairs.map((p) => [p.slice(0, p.indexOf("=")), p.slice(p.indexOf("=") + 1)]);
const secret = ec.generateSecret();
const keys = await ec.deriveKeys(secret);
const ciphertext = await ec.seal(keys, ec.encodePayload(env));
const r = await fetch(`${server}/api/v1/drops`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ ciphertext, claim_hash: keys.claimHash, ttl: 600 }),
});
if (r.status !== 201) throw new Error(`HTTP ${r.status}: ${await r.text()}`);
const { id } = await r.json();
console.log(`${server}/c/${id} ${ec.encodeSecret(secret)}`);
