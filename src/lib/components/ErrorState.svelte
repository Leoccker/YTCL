<script lang="ts">
  interface Props {
    message: string;
    onRetry?: () => void;
  }

  let { message, onRetry }: Props = $props();
</script>

<div class="error">
  <p class="message">{message}</p>
  {#if onRetry}
    <button class="retry" onclick={onRetry}>Tentar de novo</button>
  {/if}
</div>

<style>
  .error {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-6) var(--space-2);
  }
  .message {
    color: #f87171;
  }
  .retry {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
  }
  .retry:hover {
    background: var(--bg-hover);
  }
</style>
