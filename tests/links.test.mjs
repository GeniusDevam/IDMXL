import { test } from "node:test";
import assert from "node:assert/strict";
import ts from "../apps/desktop/node_modules/typescript/lib/typescript.js";
import { readFileSync } from "node:fs";
const source = readFileSync(
  new URL("../apps/desktop/src/links.ts", import.meta.url),
  "utf8",
);
const { importableLinks } = await import(
  `data:text/javascript;base64,${Buffer.from(ts.transpile(source, { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 })).toString("base64")}`
);
test("URI-list imports preserve signed links, deduplicate and exclude comments, credentials and other protocols", () => {
  assert.equal(
    importableLinks(
      "# browser export\r\nhttps://example.org/a?token=secret\r\nfile:///C:/secret\nhttps://user:password@example.org/a\nhttps://example.org/a?token=secret\nhttp://example.org/b",
    ),
    "https://example.org/a?token=secret\nhttp://example.org/b",
  );
});
test("oversized clipboard and drop payloads are rejected before review", () => {
  assert.equal(importableLinks("x".repeat(1_048_577)), "");
  assert.equal(
    importableLinks(Array(1001).fill("https://example.org/a").join("\n")),
    "",
  );
  assert.equal(importableLinks("https://example.org/" + "x".repeat(16384)), "");
  assert.equal(importableLinks("some prose https://example.org/a"), "");
});
