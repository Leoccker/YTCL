<script lang="ts">
  /**
   * Placeholder de carregamento — brilho animado em CSS, nunca um spinner
   * cobrindo a tela (regra do projeto).
   */
  interface Props {
    variant: "rows" | "cards" | "header";
    count?: number;
  }

  let { variant, count = 6 }: Props = $props();

  const items = $derived(Array.from({ length: count }));
</script>

<div class="skeleton">
  {#if variant === "rows"}
    {#each items as _, i (i)}
      <div class="row">
        <div class="shine sq"></div>
        <div class="lines">
          <div class="shine line w60"></div>
          <div class="shine line w35"></div>
        </div>
      </div>
    {/each}
  {:else if variant === "cards"}
    <div class="grid">
      {#each items as _, i (i)}
        <div class="card">
          <div class="shine sq big"></div>
          <div class="shine line w80"></div>
          <div class="shine line w50"></div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="header-skel">
      <div class="shine sq huge"></div>
      <div class="lines">
        <div class="shine line w25"></div>
        <div class="shine line w60 tall"></div>
        <div class="shine line w40"></div>
      </div>
    </div>
  {/if}
</div>

<style>
  .shine {
    background: linear-gradient(
      100deg,
      var(--bg-active) 30%,
      var(--bg-hover) 50%,
      var(--bg-active) 70%
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
    border-radius: var(--radius-sm);
  }
  @keyframes shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 56px;
    padding: 0 var(--space-3);
  }
  .sq {
    flex: none;
    width: 40px;
    height: 40px;
  }
  .lines {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    flex: 1;
  }
  .line {
    height: 10px;
  }
  .w25 {
    width: 25%;
  }
  .w35 {
    width: 35%;
  }
  .w40 {
    width: 40%;
  }
  .w50 {
    width: 50%;
  }
  .w60 {
    width: 60%;
  }
  .w80 {
    width: 80%;
  }
  .tall {
    height: 22px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--space-2);
    padding: var(--space-3);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .big {
    width: 100%;
    height: auto;
    aspect-ratio: 1;
  }

  .header-skel {
    display: flex;
    gap: var(--space-6);
    align-items: flex-end;
    padding: var(--space-6);
  }
  .huge {
    width: 180px;
    height: 180px;
  }
</style>
