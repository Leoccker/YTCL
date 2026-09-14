<script lang="ts">
  interface Props {
    title: string;
    message?: string;
  }

  let { title, message }: Props = $props();
</script>

<div class="empty">
  <p class="title">{title}</p>
  {#if message}<p class="message">{message}</p>{/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-8);
    text-align: center;
  }
  .title {
    font-size: 15px;
    color: var(--text);
  }
  .message {
    font-size: 13px;
    color: var(--text-dim);
  }
</style>
