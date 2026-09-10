<script lang="ts">
  /** Tela de conexão de conta. Aparece quando a biblioteca é pedida sem login. */
  import { auth } from "../stores/auth.svelte";

  let showManual = $state(false);
  let cookie = $state("");

  async function submitManual(e: SubmitEvent) {
    e.preventDefault();
    if (cookie.trim()) await auth.loginCookie(cookie.trim());
    if (auth.loggedIn) cookie = "";
  }
</script>

<div class="login">
  <h1>Conecte sua conta</h1>
  <p class="lead">
    A busca funciona sem conta. Para ver sua biblioteca, playlists e curtidas,
    entre com o Google.
  </p>

  <button class="primary" onclick={() => auth.loginGoogle()} disabled={auth.busy}>
    {auth.busy ? "aguardando o login…" : "Entrar com o Google"}
  </button>

  {#if auth.error}
    <p class="error">{auth.error}</p>
  {/if}

  <button class="link" onclick={() => (showManual = !showManual)}>
    {showManual ? "esconder" : "o Google bloqueou a janela?"}
  </button>

  {#if showManual}
    <form onsubmit={submitManual}>
      <p class="hint">
        Cole o cabeçalho <code>Cookie</code> de uma sessão do
        <code>music.youtube.com</code> (DevTools → Network → qualquer requisição
        → Request Headers), ou o conteúdo de um <code>cookies.txt</code>.
      </p>
      <textarea
        bind:value={cookie}
        rows="4"
        placeholder="SID=…; HSID=…; SAPISID=…"
        spellcheck="false"
      ></textarea>
      <button type="submit" disabled={auth.busy || !cookie.trim()}>
        Usar este cookie
      </button>
    </form>
  {/if}

  <p class="fineprint">
    O cookie é guardado no cofre de senhas do sistema, nunca em arquivo.
  </p>
</div>

<style>
  .login {
    max-width: 440px;
    margin: 0 auto;
    padding: var(--space-8) var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  h1 {
    margin: 0;
    font-size: 22px;
  }
  .lead {
    color: var(--text-dim);
    margin: 0;
  }
  .primary {
    align-self: flex-start;
    padding: var(--space-3) var(--space-4);
    background: var(--accent);
    color: #fff;
    border-radius: var(--radius);
    font-weight: 600;
  }
  .primary:disabled {
    opacity: 0.6;
  }
  .link {
    align-self: flex-start;
    color: var(--text-dim);
    font-size: 12px;
    text-decoration: underline;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .hint {
    font-size: 12px;
    color: var(--text-faint);
    margin: 0;
  }
  code {
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--bg-elevated);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
  }
  textarea {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    padding: var(--space-3);
    font-family: var(--font-mono);
    font-size: 12px;
    resize: vertical;
  }
  form button {
    align-self: flex-start;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
  }
  .error {
    color: #f87171;
    font-size: 13px;
    margin: 0;
  }
  .fineprint {
    font-size: 11px;
    color: var(--text-faint);
    margin: 0;
  }
</style>
