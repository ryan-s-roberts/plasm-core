const kitchen = plasm.acme.Product.query({ name: "KitchenSink" })
  .select("id", "name");

const sinks = plasm.acme.Product.query({})
  .where(field("id").eq("p1"))
  .select("id", "name");

const summary = Plan.map(kitchen, product => ({
  title: template`Summary for ${product.name}`,
}));

Plan.return({ kitchen, sinks, summary });
