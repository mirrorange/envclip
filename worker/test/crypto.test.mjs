import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { authHash, authToken, fromB64url, newId, newKey, seal, unseal } from "../public/assets/crypto.js";
import { parseDotenv } from "../public/assets/dotenv.js";

const vec = JSON.parse(readFileSync(new URL("../../testdata/v1.json", import.meta.url)));

test("v1 vector: seal is deterministic given key and nonce", async () => {
  const s = await seal(vec.vars, { id: vec.id, key: vec.key, nonce: fromB64url(vec.nonce) });
  assert.equal(s.blob, vec.blob);
  assert.equal(s.auth_hash, vec.auth_hash);
  assert.equal(await authToken(vec.key), vec.auth);
});

test("v1 vector: unseal", async () => {
  assert.deepEqual(await unseal(vec.id, vec.key, vec.blob), vec.vars);
});

test("ciphertext is bound to its id", async () => {
  await assert.rejects(unseal("zzzzzzzzzzzz", vec.key, vec.blob));
});

test("wrong key fails", async () => {
  await assert.rejects(unseal(vec.id, newKey(), vec.blob));
});

test("round trip with fresh id and key", async () => {
  const vars = [["A", ""], ["B_2", "x=y #z"]];
  const s = await seal(vars);
  assert.match(s.id, /^[0-9abcdefghjkmnpqrstvwxyz]{12}$/);
  assert.equal(fromB64url(s.key).length, 32);
  assert.equal(s.auth_hash, await authHash(s.key));
  assert.deepEqual(await unseal(s.id, s.key, s.blob), vars);
  assert.notEqual(newId(), newId());
});

test("seal rejects bad names", async () => {
  await assert.rejects(seal([["1BAD", "x"]]));
  await assert.rejects(seal([["A-B", "x"]]));
});

test("dotenv", () => {
  const text = [
    "# comment",
    "PLAIN=hello world   # trailing",
    "export EXPORTED=1",
    "EMPTY=",
    "HASH=a#b",
    "SINGLE='keep \\n raw'",
    'DOUBLE="tab\\tnew\\nquote\\""',
    'MULTI="line1',
    'line2"',
    "  SPACED = v  ",
    "",
  ].join("\r\n");
  assert.deepEqual(parseDotenv(text), [
    ["PLAIN", "hello world"],
    ["EXPORTED", "1"],
    ["EMPTY", ""],
    ["HASH", "a#b"],
    ["SINGLE", "keep \\n raw"],
    ["DOUBLE", 'tab\tnew\nquote"'],
    ["MULTI", "line1\nline2"],
    ["SPACED", "v"],
  ]);
  assert.throws(() => parseDotenv("NOPE"), /line 1/);
  assert.throws(() => parseDotenv('A="open'), /unterminated/);
  assert.throws(() => parseDotenv("1A=x"), /bad name/);
});
