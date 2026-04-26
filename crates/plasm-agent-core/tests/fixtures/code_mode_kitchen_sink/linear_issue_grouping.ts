const issues = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("name").contains("Sink"))
  .select("id", "name");

const byState = Plan.groupBy(issues, (issue) => issue.name).count("issues");

Plan.return([issues, byState]);
