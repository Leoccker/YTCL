import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { test } from "node:test";
import ts from "typescript";

// Mesmo padrão de `library-session.test.mjs`: roda a lógica real do
// componente com o IPC controlado, sem layout nem scheduler do Svelte.
const file = readFileSync(new URL("../src/lib/views/Search.svelte", import.meta.url), "utf8");
const script = file.slice(file.indexOf(">") + 1, file.indexOf("</script>"));
const ast = ts.createSourceFile("search.ts", script, ts.ScriptTarget.Latest, true);
const body = ast.statements.filter(node => !ts.isImportDeclaration(node))
  .map(node => node.getFullText(ast)).join("\n");
const code = ts.transpileModule(body, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
}).outputText;

function harness() {
  const requests = [];
  const context = vm.createContext({
    $state: value => value,
    onMount: () => {},
    search: (termo, filtro, refresh) => new Promise((resolve, reject) => {
      requests.push({ termo, filtro, refresh, resolve, reject });
    }),
    searchMore: () => new Promise(() => {}),
    errorMessage: error => String(error),
    player: { play() {} },
    router: { push() {} },
  });
  vm.runInContext(`${code}\nglobalThis.subject = {
    run,
    type: value => { query = value; },
    read: () => ({ items, loading, loadingMore, searched, error, moreError, lastTerm, continuation }),
  };`, context);
  return { ...context.subject, requests };
}

const page = items => ({
  data: { items, continuation: "cursor" },
  stale: false,
});

test("apagar o campo impede a resposta anterior de repovoar a tela", async () => {
  const h = harness();
  h.type("radiohead");
  const emVoo = h.run();
  const pedido = h.requests.shift();
  assert.equal(pedido.termo, "radiohead");
  assert.equal(h.read().loading, true);

  h.type("");
  await h.run();
  assert.equal(h.read().loading, false, "o skeleton tem que sumir junto");

  pedido.resolve(page([{ type: "track", id: "x", title: "x" }]));
  await emVoo;

  // `assert.deepEqual` compara protótipos: o array vazio nasce dentro do vm
  // e é de outro realm, então o tamanho é o que dá para comparar aqui.
  assert.equal(h.read().items.length, 0, "resultado cancelado voltou pra tela");
  assert.equal(h.read().searched, false);
  assert.equal(h.read().continuation, null);
  assert.equal(h.read().lastTerm, "");
});

test("erro de busca cancelada não aparece depois de limpar o campo", async () => {
  const h = harness();
  h.type("radiohead");
  const emVoo = h.run();
  const pedido = h.requests.shift();

  h.type("");
  await h.run();

  pedido.reject(new Error("rede caiu"));
  await emVoo;

  assert.equal(h.read().error, null, "erro de pedido cancelado foi parar na tela");
  assert.equal(h.read().loading, false);
});

test("a busca seguinte continua funcionando depois de limpar", async () => {
  const h = harness();
  h.type("radiohead");
  const emVoo = h.run();
  const antigo = h.requests.shift();

  h.type("");
  await h.run();

  h.type("portishead");
  const nova = h.run();
  const atual = h.requests.shift();
  assert.equal(atual.termo, "portishead");

  // A antiga responde depois da nova: a nova é que tem que ficar.
  atual.resolve(page([{ type: "track", id: "novo", title: "novo" }]));
  await nova;
  antigo.resolve(page([{ type: "track", id: "antigo", title: "antigo" }]));
  await emVoo;

  assert.equal(h.read().items.length, 1);
  assert.equal(h.read().items[0].id, "novo");
  assert.equal(h.read().lastTerm, "portishead");
});
