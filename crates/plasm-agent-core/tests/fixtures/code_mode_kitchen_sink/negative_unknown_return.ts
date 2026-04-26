const products = plasm.acme.Product.query({ category: "kitchen" })
  .select("id");

Plan.return([products, "not_a_node" as any]);
