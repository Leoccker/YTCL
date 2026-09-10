/**
 * Estado de autenticação, compartilhado pela UI.
 *
 * Uma instância só (exportada como `auth`). Os componentes leem `auth.session`
 * / `auth.accounts` e chamam os métodos; a reatividade das runes propaga.
 */
import {
  authStatus,
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

  readonly loggedIn = $derived(this.session.kind === "active");
  readonly expired = $derived(this.session.kind === "expired");
  readonly account = $derived(
    this.session.kind === "logged_out" ? null : this.session.account,
  );

  async refresh() {
    try {
      const s = await authStatus();
      this.session = s.session;
      this.accounts = s.accounts;
    } catch (e) {
      this.error = errorMessage(e);
    }
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
