export type Cancion = {
  clave: string;
  titulo: string;
  artista: string;
  duracion: number;
  caratula: string;
  colores: string[];
  esVideo: boolean;
};
export type VideoApple = { id: string; nombre: string };

type Busqueda = { calidad: string; busTipo: string; busExigencia: string; busExcluir: string; ytCookies: string };

const applePorClave = new Map<string, VideoApple | null>();

export function musicKit(): any {
  try {
    return (globalThis as any).MusicKit?.getInstance?.() ?? null;
  } catch {
    return null;
  }
}

export function aCancion(item: any): Cancion | null {
  if (!item) return null;
  const a = item.attributes ?? {};
  const titulo = a.name ?? item.title ?? "";
  if (!titulo) return null;
  const artista = a.artistName ?? item.artistName ?? "";
  const arte = a.artwork ?? item.artwork ?? {};
  return {
    clave: String(a.playParams?.catalogId ?? item.songId ?? item.id ?? `${artista}|${titulo}`),
    titulo,
    artista,
    duracion: (a.durationInMillis ?? item.playbackDuration ?? 0) / 1000,
    caratula: typeof arte.url === "string" ? arte.url.replace("{w}", "1200").replace("{h}", "1200").replace("{f}", "jpg") : "",
    colores: [arte.bgColor, arte.textColor1, arte.textColor2].filter((c: unknown) => typeof c === "string").map((c: string) => `#${c}`),
    esVideo: item.type === "musicVideo" || a.playParams?.kind === "musicVideo",
  };
}

export function consultaMv(c: Cancion, cfg: Busqueda) {
  return new URLSearchParams({
    key: c.clave,
    artist: c.artista,
    title: c.titulo,
    duration: String(c.duracion || ""),
    quality: cfg.calidad,
    prefer: cfg.busTipo,
    strict: cfg.busExigencia,
    exclude: cfg.busExcluir,
    cookies: cfg.ytCookies,
  });
}

function simplificar(texto: string) {
  return texto
    .toLowerCase()
    .replace(/[(（\[【].*$/, "")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

/** Videoclip de Apple Music que corresponde a la canción, si el catálogo tiene uno de la misma versión. */
export async function appleDe(c: Cancion): Promise<VideoApple | null> {
  if (applePorClave.has(c.clave)) return applePorClave.get(c.clave) ?? null;
  const mk = musicKit();
  if (!mk?.api?.v3?.music) return null;
  try {
    const r = await mk.api.v3.music(`/v1/catalog/${mk.storefrontId}/search`, {
      term: `${c.artista} ${c.titulo}`,
      types: "music-videos",
      limit: 6,
    });
    const titulo = simplificar(c.titulo);
    const artista = simplificar(c.artista.split(/,|&| feat/i)[0]);
    // Otra versión de la canción (en directo, acústica, remix) no vale como su videoclip.
    const otraVersion = /live|vivo|directo|ac[uú]sti|acoust|remix|cover|sesi[oó]n|session|karaoke|instrumental|versi[oó]n|version|ライブ/i;
    const mejor = (r?.data?.results?.["music-videos"]?.data ?? []).find((v: any) => {
      const crudo: string = v.attributes?.name ?? "";
      const duracion = (v.attributes?.durationInMillis ?? 0) / 1000;
      if (otraVersion.test(crudo) && !otraVersion.test(c.titulo)) return false;
      if (c.duracion && duracion && Math.abs(duracion - c.duracion) > 25) return false;
      return titulo && simplificar(crudo) === titulo && simplificar(v.attributes?.artistName ?? "").includes(artista);
    });
    const hallado = mejor ? { id: String(mejor.id), nombre: mejor.attributes.name } : null;
    applePorClave.set(c.clave, hallado);
    return hallado;
  } catch {
    return null;
  }
}
