// Minimal .env parser. Mirrors cli/src/dotenv.rs.
//   NAME=value            # inline comments need a space before '#'
//   export NAME=value
//   NAME='literal, may span lines'
//   NAME="escapes \n \t \" \\ allowed, may span lines"

const ESCAPES = { n: "\n", r: "\r", t: "\t", '"': '"', "\\": "\\" };

export function parseDotenv(text) {
  const out = [];
  const s = text.replace(/\r\n?/g, "\n");
  let i = 0;
  let line = 1;
  const eol = () => {
    while (i < s.length && s[i] !== "\n") i++;
  };

  while (i < s.length) {
    const c = s[i];
    if (c === "\n") {
      i++;
      line++;
      continue;
    }
    if (c === " " || c === "\t") {
      i++;
      continue;
    }
    if (c === "#") {
      eol();
      continue;
    }

    const start = line;
    let j = i;
    while (j < s.length && s[j] !== "=" && s[j] !== "\n") j++;
    if (s[j] !== "=") throw new Error(`line ${start}: expected NAME=value`);
    const name = s.slice(i, j).trim().replace(/^export\s+/, "");
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) throw new Error(`line ${start}: bad name "${name}"`);
    i = j + 1;
    while (s[i] === " " || s[i] === "\t") i++;

    let value = "";
    const q = s[i];
    if (q === "'" || q === '"') {
      i++;
      for (;;) {
        if (i >= s.length) throw new Error(`line ${start}: unterminated quote`);
        const ch = s[i++];
        if (ch === q) break;
        if (ch === "\n") line++;
        if (q === '"' && ch === "\\" && s[i] in ESCAPES) value += ESCAPES[s[i++]];
        else value += ch;
      }
      eol();
    } else {
      j = i;
      while (j < s.length && s[j] !== "\n") j++;
      value = s.slice(i, j).replace(/\s+#.*$/, "").trim();
      i = j;
    }
    out.push([name, value]);
  }
  return out;
}
