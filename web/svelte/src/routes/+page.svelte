<script>
  import { onMount } from 'svelte';
  import { page } from '$app/stores';

  onMount(() => {
    // Redirect to /ripper as per PRODUCT.md specification
    if ($page.url.pathname === '/') {
      window.location.replace('/ripper');
    }
  });
</script>

<!-- Redirecting to /ripper... -->
