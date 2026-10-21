// One frame of the hero's live scene, for scripts/hero-posters.ts: it renders once the bark has
// loaded, then stops, so the page can be captured as the poster.
import { mount } from "@/components/landing/tree-scene";
import { HERO } from "@/lib/hero-tree";

const canvas = document.querySelector("canvas")!;
const scene = mount(
  canvas,
  () => {
    scene.run(false);
    document.title = "ready";
  },
  HERO,
);
scene.run(true);
