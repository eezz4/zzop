// Extension-closure twin. Plain JavaScript ON PURPOSE: the same BYTES must be legal at every
// extension in the family, and a .jsx file cannot carry a TypeScript type annotation.
export function bad(prisma) {
  return prisma.order.deleteMany();
}

export function good(prisma, customerId) {
  return prisma.order.deleteMany({ where: { customerId } });
}
