/**
 * Estado de autenticação, compartilhado pela UI.
 *
 * Uma instância só (exportada como `auth`). Os componentes leem `auth.session`
 * / `auth.accounts` e chamam os métodos; a reatividade das runes propaga.
 */
import {
  authStatus,
  onAuthSession,
  authLoginGoogle,
  authLoginCookie,
  authReconnect,
  authSwitch,
  authLogout,
  authRename,
  errorMessage,
  type Account,
  type Session,
} from "../api";

class AuthState {
  session = $state<Session>({ kind: "logged_out" });
  accounts = $state<Account[]>([]);
  /** Uma operação de login/reconexão em andamento (webview aberto). */
  busy = $state(false);
  error = $state<string | null>(null);
  #initialized = false;
  #refreshVersion = 0;

  readonly loggedIn = $derived(this.session.kind === "active");
  readonly expired = $derived(this.session.kind === "expired");
  readonly account = $derived(
    this.session.kind === "logged_out" ? null : this.session.account,
  );

  async refresh() {
    const version = ++this.#refreshVersion;
    try {
      const s = await authStatus();
      if (version !== this.#refreshVersion) return;
      this.session = s.session;
      this.accounts = s.accounts;
    } catch (e) {
      if (version !== this.#refreshVersion) return;
      this.error = errorMessage(e);
    }
  }

  /**
   * Assina antes do snapshot. A sessão pode ser hidratada a qualquer momento
   * durante o boot; se o evento ja aconteceu, `refresh` le o estado atual.
   */
  async init() {
    if (this.#initialized) return;
    this.#initialized = true;
    await onAuthSession((session) => {
      this.session = session;
      void this.refresh();
    });
    await this.refresh();
  }

  private async run(fn: () => Promise<unknown>) {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      await fn();
      await this.refresh();
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.busy = false;
    }
  }

  loginGoogle() {
    return this.run(authLoginGoogle);
  }

  loginCookie(cookie: string) {
    return this.run(() => authLoginCookie(cookie));
  }

  reconnect() {
    return this.run(authReconnect);
  }

  switch(id: string) {
    return this.run(() => authSwitch(id));
  }

  logout(id: string) {
    return this.run(() => authLogout(id));
  }

  async rename(id: string, label: string) {
    try {
      this.accounts = await authRename(id, label);
    } catch (e) {
      this.error = errorMessage(e);
    }
  }
}

export const auth = new AuthState();
