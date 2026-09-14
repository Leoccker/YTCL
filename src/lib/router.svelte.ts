/**
 * Roteador em memória: pilha de navegação com voltar/avançar, como um
 * navegador de verdade. Sem URL nem `history` do DOM — o app é uma janela
 * só, não há nada para sincronizar com o SO.
 */

export type Route =
  | { name: "home" }
  | { name: "search" }
  | { name: "library" }
  | { name: "queue" }
  | { name: "album"; id: string }
  | { name: "artist"; id: string }
  | { name: "playlist"; id: string };

/** Teto da pilha: navegação é em memória, não pode crescer sem limite. */
const MAX_STACK = 50;

function sameRoute(a: Route, b: Route): boolean {
  if (a.name !== b.name) return false;
  if ("id" in a && "id" in b) return a.id === b.id;
  return true;
}

class Router {
  #stack = $state<Route[]>([{ name: "home" }]);
  #index = $state(0);

  readonly current = $derived(this.#stack[this.#index]!);
  readonly canBack = $derived(this.#index > 0);
  readonly canForward = $derived(this.#index < this.#stack.length - 1);

  /**
   * Empilha uma rota nova. Descarta o "futuro" (como clicar num link depois
   * de voltar, num navegador) e ignora push idêntico à rota atual — senão
   * clicar duas vezes no mesmo álbum criaria duas entradas na pilha.
   */
  push(route: Route) {
    if (sameRoute(this.current, route)) return;

    const kept = this.#stack.slice(0, this.#index + 1);
    let next = [...kept, route];
    if (next.length > MAX_STACK) {
      next = next.slice(next.length - MAX_STACK);
    }
    this.#stack = next;
    this.#index = next.length - 1;
  }

  back() {
    if (this.canBack) this.#index--;
  }

  forward() {
    if (this.canForward) this.#index++;
  }
}

export const router = new Router();
