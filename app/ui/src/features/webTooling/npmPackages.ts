/** The npm packages of the project (for Bun's scripts in Run Anything). */
import { npmApi, type NpmPackage } from "@/ipc/web";

export const npmPackages = (): Promise<NpmPackage[]> => npmApi.packages().catch(() => []);
