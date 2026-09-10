<script lang="ts">
  /**
   * Barra não modal, no topo, quando o YouTube rejeita o cookie da conta.
   * Não interrompe nada — busca e (na fase 3) reprodução seguem funcionando;
   * só a biblioteca fica indisponível até reconectar.
   */
  import { auth } from "../stores/auth.svelte";
</script>

{#if auth.expired && auth.account}
  <div class="banner">
    <span>
      A sessão de <strong>{auth.account.label}</strong> expirou.
    </span>
    <button onclick={() => auth.reconnect()} disabled={auth.busy}>
      {auth.busy ? "abrindo…" : "Reconectar"}
    </button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    background: var(--accent-dim);
    color: #fff;
    font-size: 13px;
  }
  button {
    margin-left: auto;
    padding: var(--space-1) var(--space-3);
    border: 1px solid rgb(255 255 255 / 0.5);
    border-radius: var(--radius-sm);
    color: #fff;
  }
  button:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.15);
  }
  button:disabled {
    opacity: 0.6;
  }
</style>
