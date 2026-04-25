// Code Mode builds one agent-facing Plan. The host sees only the typed Plan DAG,
// then returns dry-run or execution results to the agent.
const products = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("id").eq("p1"))
  .select("id", "name");

Plan.return({ products });
