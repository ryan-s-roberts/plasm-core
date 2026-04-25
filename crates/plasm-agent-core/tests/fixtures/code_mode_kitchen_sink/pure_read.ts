const products = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("id").eq("p1"))
  .select("id", "name");

Plan.return({ products });
