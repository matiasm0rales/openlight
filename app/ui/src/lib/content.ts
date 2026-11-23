// Carga el contenido editable de `src/content/` según el idioma.
import type { HelpTopic, Region } from "./bindings";
import es from "../content/guide/es.json";
import en from "../content/guide/en.json";
import { i18n } from "./i18n/index.svelte";

export interface GuideTopic {
  title: string;
  intro: string;
  steps: string[];
  tip?: string;
  links?: { label: string; url: string }[];
}

export interface WizardGuide {
  lead: string;
  steps: string[];
  image?: string;
  note?: string;
  phone_steps?: string[];
  link?: { label: string; url: string };
}

export interface Guide {
  topics: Record<HelpTopic, GuideTopic>;
  wizard: Record<"tuya_account" | "cloud_project" | "link_app" | "credentials", WizardGuide>;
  data_centers: { where: string; region: Region }[];
}

const GUIDES: Record<string, Guide> = { es: es as Guide, en: en as Guide };

export function guide(): Guide {
  return GUIDES[i18n.locale] ?? GUIDES.es;
}

const images = import.meta.glob("../content/img/*.{svg,png}", { eager: true, query: "?url", import: "default" }) as Record<string, string>;

export function imageUrl(name: string | undefined): string | undefined {
  if (!name) return undefined;
  return Object.entries(images).find(([path]) => path.split("/").pop()?.replace(/\.(svg|png)$/, "") === name)?.[1];
}
