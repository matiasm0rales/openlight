// Contra qué backend corre la UI. Lo decide `main.ts` antes de montarla.
export const runtime = {
  /** Backend falso (navegador, Playwright): hay cosas que solo tienen sentido ahí. */
  mock: false,
};
