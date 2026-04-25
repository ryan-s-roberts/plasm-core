const products = plasm.acme.Product.query({ category: "kitchen" })
  .select("id");

Plan.return({ products, missing: "not_a_node" });
