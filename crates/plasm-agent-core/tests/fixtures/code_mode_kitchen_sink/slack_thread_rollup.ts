const messages = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("id").contains("p"))
  .select("id", "name");

const threadDigest = Plan.project(messages, {
  thread: (message) => message.id,
  author: (message) => message.name,
});

const byUser = Plan.groupBy(messages, (message) => message.name).count("messages");

Plan.return([messages, threadDigest, byUser]);
