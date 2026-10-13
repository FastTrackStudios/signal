// The stage the app is drawn on: the iPad frame, scaled to fit the window
// or to the iPad's real size. Anything that floats over the app (menus)
// draws into the stage, in its coordinates, so it scales with it.

import { createContext, useContext } from "react";

export interface Stage {
  el: HTMLElement | null;
  scale: number;
}

export const StageCtx = createContext<Stage>({ el: null, scale: 1 });

export function useStage(): Stage {
  return useContext(StageCtx);
}
