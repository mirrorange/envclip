// npm test — checks the browser crypto against the Rust vectors.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as ec from "../public/crypto.js";

test("known vector matches cli/src/crypto.rs", async () => {
  const secret = new Uint8Array(16).fill(0x42);
  assert.equal(ec.encodeSecret(secret), "89144gj289144gj289144gj288");
  const keys = await ec.deriveKeys(secret);
  assert.equal(keys.claimToken, "5afZeGlOYEASJ0SfNwMNRCBmGES_1QpG7JIxmyAPvS8");
  assert.equal(keys.claimHash, "7d6867f5035879db744c92ef7ddf44a1d87f68ed2be2d14a73dc6a3037585534");
});

test("secret encode/parse roundtrip", () => {
  for (let i = 0; i < 100; i++) {
    const s = ec.generateSecret();
    const enc = ec.encodeSecret(s);
    assert.equal(enc.length, 26);
    assert.deepEqual(ec.parseSecret(enc), s);
    assert.deepEqual(ec.parseSecret(enc.toUpperCase().replace(/(.{4})/g, "$1-")), s);
  }
  assert.equal(ec.parseSecret("short"), null);
  assert.equal(ec.parseSecret("u".repeat(26)), null);
});

test("seal/open with padding", async () => {
  const keys = await ec.deriveKeys(ec.generateSecret());
  const pt = ec.encodePayload([["A", "1"]]);
  const sealed = await ec.seal(keys, pt);
  const out = await ec.open(keys, sealed);
  assert.equal(out.length, 256);
  assert.deepEqual(JSON.parse(new TextDecoder().decode(out)), { v: 1, env: [["A", "1"]] });
  const other = await ec.deriveKeys(ec.generateSecret());
  await assert.rejects(ec.open(other, sealed));
});

test("dotenv parsing mirrors the CLI", () => {
  const src = "# c\nA=1\nexport B = 'x y' \nC=\"l1\\nl2\"\nD=\"multi\nline\"\nE=plain # note\nA=2\n";
  assert.deepEqual(ec.parseDotenv(src), [
    ["A", "2"],
    ["B", "x y"],
    ["C", "l1\nl2"],
    ["D", "multi\nline"],
    ["E", "plain"],
  ]);
  assert.throws(() => ec.parseDotenv("1A=x"));
  assert.throws(() => ec.parseDotenv("NOEQ"));
});
