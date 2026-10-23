import { useEffect, useState } from "react";
import type { LibraryApi } from "../library/types";

/** A drawn cover, with real covers on top when the library has them. One cover fills the square;
 * two sit side by side; three put one on top and two below; four make a grid. A cover that fails
 * to load is left out, and with none left the drawn cover shows. */
export function Artwork({ color, large = false, src = null, srcs }: {
  color: number;
  large?: boolean;
  src?: string | null;
  srcs?: string[];
}) {
  const [failed, setFailed] = useState<ReadonlySet<string>>(() => new Set());
  const images = (srcs ?? (src ? [src] : [])).filter((url) => !failed.has(url)).slice(0, 4);
  return <div aria-hidden="true" className={`artwork art-${color % 8}${large ? " artwork-large" : ""}`}>
    <span className="art-orbit" /><span className="art-line" /><span className="art-grain" />
    {images.length > 0 && <div className={`art-collage art-collage-${images.length}`}>
      {images.map((url) => <img key={url} className="art-image" src={url} alt="" loading="lazy" decoding="async" draggable={false}
        onError={() => setFailed((before) => new Set(before).add(url))} />)}
    </div>}
  </div>;
}

/** An artist's cover: a collage of their releases' covers. */
export function ArtistArtwork({ api, artistId, color, large = false }: { api: LibraryApi; artistId: number; color: number; large?: boolean }) {
  const [srcs, setSrcs] = useState<string[]>([]);
  useEffect(() => {
    let stopped = false;
    setSrcs([]);
    void api.artistCovers?.(artistId).then((ids) => {
      if (stopped) return;
      setSrcs(ids.map((id) => api.artworkUrl?.(id)).filter((url): url is string => typeof url === "string"));
    });
    return () => { stopped = true; };
  }, [api, artistId]);
  return <Artwork color={color} large={large} srcs={srcs} />;
}
