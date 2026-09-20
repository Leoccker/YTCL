import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { test } from "node:test";
import ts from "typescript";

// Executa a lógica real do componente, com IPC controlado. Não simula layout
// nem o scheduler do Svelte; o efeito de sessão é disparado explicitamente.
const file = readFileSync(new URL("../src/lib/views/Library.svelte", import.meta.url), "utf8");
const script = file.slice(file.indexOf(">") + 1, file.indexOf("</script>"));
const ast = ts.createSourceFile("library.ts", script, ts.ScriptTarget.Latest, true);
const body = ast.statements.filter(node => !ts.isImportDeclaration(node))
  .map(node => node.getFullText(ast)).join("\n");
const code = ts.transpileModule(body, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
}).outputText;

function harness() {
  const effects = [];
  const requests = [];
  const request = (tab, cursor = null) => new Promise((resolve, reject) => {
    requests.push({ tab, cursor, resolve, reject });
  });
  const auth = {
    session: { kind: "logged_out" },
    get loggedIn() { return this.session.kind === "active"; },
  };
  const context = vm.createContext({
    auth, $state: value => value, $effect: fn => effects.push(fn), untrack: fn => fn(),
    libraryPlaylists: () => request("playlists"),
    libraryAlbums: () => request("albums"),
    libraryArtists: () => request("artists"),
    likedSongs: cursor => request("liked", cursor),
    errorMessage: error => String(error),
  });
  vm.runInContext(`${code}\nglobalThis.subject = {
    load, moreLiked,
    select: value => { tab = value; },
    read: () => ({ playlists, albums, artists, liked, likedCont, st }),
  };`, context);
  return {
    ...context.subject, auth, requests,
    session(id) {
      auth.session = id ? { kind: "active", account: { id } } : { kind: "logged_out" };
      effects.forEach(fn => fn());
    },
  };
}

const flush = () => new Promise(resolve => setImmediate(resolve));

test("troca Active(A) -> Active(B) limpa listas e busca a conta B", async () => {
  const h = harness();
  h.session("A");
  h.requests.shift().resolve([{ id: "playlist-A" }]);
  await flush();
  assert.equal(h.read().playlists[0].id, "playlist-A");
  h.session("B");
  assert.equal(h.read().playlists.length, 0);
  assert.equal(h.requests.length, 1);
  h.requests.shift().resolve([{ id: "playlist-B" }]);
  await flush();
  assert.equal(h.read().playlists[0].id, "playlist-B");
});

test("resposta de primeira página de outra aba não sobrevive à troca", async () => {
  const h = harness();
  h.session("A");
  const old = h.requests.shift();
  h.select("albums");
  h.session("B");
  const next = h.requests.shift();
  old.resolve([{ id: "playlist-A" }]);
  next.resolve([{ id: "album-B" }]);
  await flush();
  assert.equal(h.read().playlists.length, 0);
  assert.equal(h.read().st.playlists.loaded, false);
  assert.equal(h.read().albums[0].id, "album-B");
});

test("página atrasada de curtidas não altera itens, cursor ou loading de B", async () => {
  const h = harness();
  h.select("liked");
  h.session("A");
  h.requests.shift().resolve({ items: [{ id: "A1" }], continuation: "cursor-A" });
  await flush();
  const oldMore = h.moreLiked();
  const old = h.requests.shift();
  assert.equal(old.cursor, "cursor-A");
  h.session("B");
  const firstB = h.requests.shift();
  assert.equal(h.read().likedCont, null);
  old.resolve({ items: [{ id: "A2" }], continuation: "cursor-A2" });
  await oldMore;
  assert.equal(h.read().liked.length, 0);
  assert.equal(h.read().likedCont, null);
  assert.equal(h.read().st.liked.loading, true);
  firstB.resolve({ items: [{ id: "B1" }], continuation: "cursor-B" });
  await flush();
  const moreB = h.moreLiked();
  const requestB = h.requests.shift();
  assert.equal(requestB.cursor, "cursor-B");
  requestB.resolve({ items: [{ id: "B2" }], continuation: null });
  await moreB;
  assert.deepEqual(Array.from(h.read().liked, track => track.id), ["B1", "B2"]);
});

test("erro atrasado não contamina a nova conta e logout invalida a resposta", async () => {
  const h = harness();
  h.select("liked");
  h.session("A");
  const old = h.requests.shift();
  h.session("B");
  const next = h.requests.shift();
  old.reject(new Error("erro de A"));
  await flush();
  assert.equal(h.read().st.liked.error, null);
  assert.equal(h.read().st.liked.loading, true);
  h.session(null);
  next.resolve({ items: [{ id: "B" }], continuation: "cursor-B" });
  await flush();
  assert.equal(h.read().liked.length, 0);
  assert.equal(h.read().likedCont, null);
  assert.equal(h.read().st.liked.loading, false);
});

test("reconectar a mesma conta não aceita resposta da sessão anterior", async () => {
  const h = harness();
  h.session("A");
  const old = h.requests.shift();
  h.session(null);
  h.session("A");
  const next = h.requests.shift();
  next.resolve([{ id: "nova" }]);
  await flush();
  old.resolve([{ id: "antiga" }]);
  await flush();
  assert.equal(h.read().playlists[0].id, "nova");
});

test("resposta é rejeitada mesmo antes de o efeito processar a nova conta", async () => {
  const h = harness();
  h.session("A");
  const old = h.requests.shift();
  h.auth.session = { kind: "active", account: { id: "B" } };
  old.resolve([{ id: "A" }]);
  await flush();
  assert.equal(h.read().playlists.length, 0);
});
