import { call, nativeList, nativeLogin, tr } from "./api";
import { toAlbum, toArtist, toTrack } from "./catalog";
import { ALBUM_PAGE, ALBUM_TITLES, PAGE_SIZE } from "./constants";
import { albumList } from "./discovery";
import type { Artist, BrowseResult, BrowseSection } from "./types";
import { fail } from "./utils";

export function albumTitle(type: string) {
  return tr(ALBUM_TITLES[type] || "Albums");
}

export async function browse({ page }: { page?: string }): Promise<BrowseResult> {
  const target = String(page || "");
  if (target === "artists" || target.indexOf("artists:") === 0) {
    const offset = Number(target.split(":")[1] || 0);
    const auth = await nativeLogin();
    const entries = await nativeList(auth.token, "/api/artist", {
      missing: false,
      _sort: "name",
      _order: "ASC",
      _start: offset,
      _end: offset + PAGE_SIZE,
    });
    const artists: Artist[] = entries.map((artist) => toArtist({ ...artist, coverArt: "ar-" + artist.id }));
    const section: BrowseSection = { artists };
    if (entries.length === PAGE_SIZE) section.morePage = "artists:" + (offset + PAGE_SIZE);
    return { title: tr("Artists"), sections: [section] };
  }
  if (target === "genres") {
    const data = await call("getGenres");
    const genres = (data.genres && data.genres.genre) || [];
    return {
      title: tr("Genres"),
      sections: [{ links: genres.map((genre: any) => ({ title: genre.value, page: "genre:" + genre.value })) }],
    };
  }
  if (target.indexOf("genre:") === 0) {
    const name = target.slice(6);
    const results = await Promise.all([
      call("getSongsByGenre", { genre: name, count: 100 }),
      call("getAlbumList2", { type: "byGenre", genre: name, size: ALBUM_PAGE }),
    ]);
    const sections: BrowseSection[] = [];
    const albums = albumList(results[1]);
    if (albums.length) sections.push({ title: tr("Albums"), albums: albums.map(toAlbum) });
    const songs = (results[0].songsByGenre && results[0].songsByGenre.song) || [];
    if (songs.length) sections.push({ title: tr("Songs"), tracks: songs.map(toTrack) });
    return { title: name, sections };
  }
  if (target.indexOf("albums:") === 0) {
    const parts = target.split(":");
    const type = parts[1];
    const offset = Number(parts[2] || 0);
    const data = await call("getAlbumList2", { type, size: ALBUM_PAGE, offset });
    const albums = albumList(data).map(toAlbum);
    const section: BrowseSection = { albums };
    if (albums.length === ALBUM_PAGE) {
      section.morePage = "albums:" + type + ":" + (offset + ALBUM_PAGE);
    }
    return { title: albumTitle(type), sections: [section] };
  }
  throw fail("unavailable", tr("No such page."));
}
