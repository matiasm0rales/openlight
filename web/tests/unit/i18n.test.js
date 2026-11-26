import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EN, TEXTS, pickLanguage } from "../../public/js/i18n.js";

const html = readFileSync(new URL("../../public/index.html", import.meta.url), "utf8");

// Claves que la página pide: data-i18n, data-i18n-html y data-i18n-attr="atributo:clave".
const used = new Set([
  ...[...html.matchAll(/data-i18n(?:-html)?="([^"]+)"/g)].map((m) => m[1]),
  ...[...html.matchAll(/data-i18n-attr="([^"]+)"/g)].flatMap((m) => m[1].split(";").map((p) => p.split(":")[1])),
]);

test("todo texto de la página tiene traducción al inglés", () => {
  assert.ok(used.size > 50);
  assert.deepEqual([...used].filter((key) => !EN[key]), []);
});

test("no quedan traducciones que la página no use", () => {
  assert.deepEqual(Object.keys(EN).filter((key) => !used.has(key)), []);
});

test("los textos que arma el JavaScript existen en los dos idiomas", () => {
  assert.deepEqual(Object.keys(TEXTS.en).sort(), Object.keys(TEXTS.es).sort());
  assert.deepEqual(Object.keys(TEXTS.en.formats).sort(), Object.keys(TEXTS.es.formats).sort());
});

test("el aviso de beta está tal cual se pidió", () => {
  const plain = html.replace(/<[^>]+>/g, "");
  assert.ok(
    plain.includes(
      "Windows y Linux están en beta: compilan y pasan las pruebas automáticas, pero todavía no se probaron con focos reales.",
    ),
  );
});

test("el idioma sale del enlace, después de lo elegido y al final del navegador", () => {
  assert.equal(pickLanguage({ search: "?lang=en", saved: "es", language: "es-AR" }), "en");
  assert.equal(pickLanguage({ search: "", saved: "en", language: "es-AR" }), "en");
  assert.equal(pickLanguage({ search: "?lang=fr", saved: null, language: "en-US" }), "en");
});

test("por defecto es español, salvo que el navegador esté en inglés", () => {
  assert.equal(pickLanguage({ language: "es-AR" }), "es");
  assert.equal(pickLanguage({ language: "pt-BR" }), "es");
  assert.equal(pickLanguage({ language: "en-GB" }), "en");
  assert.equal(pickLanguage({}), "es");
});
