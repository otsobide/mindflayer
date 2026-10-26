/**
 * Nuxt UI's colours plugin, with nothing to do (vite.config.ts points the
 * plugin's file here).
 *
 * The original builds the palette's CSS variables at run time and writes them
 * into a `<style>` element, which a CSP without `'unsafe-inline'` refuses, so
 * the panel would lose every colour but the neutrals. main.css declares the
 * same variables in the stylesheet, where the CSP lets them through; a nonce
 * per launch would have let the element in, at the price of the server
 * rewriting the page each time it is sent.
 */
export default {
  install() {}
}
