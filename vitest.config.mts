import { defineConfig } from "vitest/config";
import { TestSequencer } from "vitest/node";

class CustomSequencer implements TestSequencer {
  shard;
  sort(files) {
    const order = [
      "setup.test.ts",
      "vft.test.ts",
      "balance.test.ts",
      "checkers.test.ts",
      "manager.test.ts",
    ];

    return [...files].sort((a, b) => {
      const aPath = a.moduleId || String(a);
      const bPath = b.moduleId || String(b);

      const aIndex = order.findIndex((name) => aPath.includes(name));
      const bIndex = order.findIndex((name) => bPath.includes(name));

      // If both are in the order list, sort by order
      if (aIndex !== -1 && bIndex !== -1) {
        return aIndex - bIndex;
      }

      // If only a is in the order list, it comes first
      if (aIndex !== -1) return -1;

      // If only b is in the order list, it comes first
      if (bIndex !== -1) return 1;

      // Otherwise, sort alphabetically
      return aPath.localeCompare(bPath);
    });
  }
}

export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
    exclude: [
      "test/vft/vft.test.ts",
    //  "test/mandelbrot/manager.test.ts",
    ],
    globals: true,
    sequence: {
      sequencer: CustomSequencer,
      concurrent: false,
    },
    fileParallelism: false,
    testTimeout: 240_000,
    setupFiles: ["./test/vitest.setup.ts"],
    reporters: ["default", "html"],
  },
});
