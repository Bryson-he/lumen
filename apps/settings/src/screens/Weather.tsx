// Weather settings: switching it on, choosing the place, the units, and
// seeing when it last updated. This is the one part of Lumen that goes
// online, so it says plainly when and what for.

import { useEffect, useId, useState } from "react";
import { useLumen } from "../store";
import type { Place, WeatherConfig } from "../types";
import { Button, Check, Segmented } from "../ui";

export function WeatherSettings() {
  const { weather, setWeather, request } = useLumen();
  const searchId = useId();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Place[] | null>(null);
  const [searching, setSearching] = useState(false);
  // A different place was chosen elsewhere: the old results no longer apply.
  const placeName = weather?.config.place?.name ?? null;
  useEffect(() => setResults(null), [placeName]);

  if (!weather) return <p className="muted small-13">Weather is not available right now.</p>;
  const { config, data, error } = weather;
  const change = (patch: Partial<WeatherConfig>) => void setWeather({ ...config, ...patch });

  const search = async () => {
    if (query.trim().length < 2) return;
    setSearching(true);
    const found = await request<Place[]>("searchPlaces", { query: query.trim() });
    setSearching(false);
    setResults(found ?? []);
  };

  return (
    <div className="weather-settings">
      <Check checked={config.enabled} onChange={(enabled) => change({ enabled })}>
        Show the weather
      </Check>
      <p className="muted small-13">
        This is the only thing Lumen goes online for. While it is on and a weather widget or bar module is showing, Lumen asks Open-Meteo for the forecast about every 20 minutes. Only the place's map position and your choice of units are sent, never your name or
        anything about your PC. Nothing is asked while a fullscreen game is in front.
      </p>

      <div className="field">
        <label htmlFor={searchId} className="small-14">
          Place
        </label>
        {config.place && (
          <span className="weather-place">
            <strong>{config.place.name}</strong>
            <button type="button" className="link small-13" onClick={() => change({ place: null })}>
              Clear
            </button>
          </span>
        )}
        <span className="parts-naming">
          <input
            id={searchId}
            className="parts-text"
            type="text"
            maxLength={80}
            placeholder={config.place ? "Search for another place" : "Search for a town or city"}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && void search()}
          />
          <Button size="bar" disabled={searching || query.trim().length < 2} onClick={() => void search()}>
            {searching ? "Searching…" : "Search"}
          </Button>
        </span>
        {!config.place && <span className="muted small">Searching sends the words you type to Open-Meteo's place search.</span>}
        {results && results.length === 0 && <span className="muted small-13">No places found. Check the spelling, or try a nearby larger town.</span>}
        {results && results.length > 0 && (
          <ul className="weather-results">
            {results.map((place) => (
              <li key={`${place.name}-${place.latitude}-${place.longitude}`}>
                <button
                  type="button"
                  className="menu-item"
                  onClick={() => {
                    change({ place, enabled: true });
                    setQuery("");
                    setResults(null);
                  }}
                >
                  {place.name}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="field">
        <span className="small-14">Pictures</span>
        <Segmented
          label="Weather pictures"
          look="grid-2"
          value={config.pictures ?? "colour"}
          onChange={(pictures) => change({ pictures })}
          options={[
            { value: "colour", label: "Colour" },
            { value: "plain", label: "Plain, one colour" },
          ]}
        />
        <span className="muted small">Plain pictures are drawn in each part's own colour, so they follow your theme.</span>
      </div>

      <div className="field">
        <span className="small-14">Units</span>
        <Segmented
          label="Units"
          look="grid-2"
          value={config.units}
          onChange={(units) => change({ units })}
          options={[
            { value: "celsius", label: "°C · km/h" },
            { value: "fahrenheit", label: "°F · mph" },
          ]}
        />
      </div>

      <div className="field-row wrap">
        <span className={`small-13 ${error ? "weather-problem" : "muted"}`} role="status">
          {!config.enabled
            ? "Weather is off."
            : !config.place
              ? "Choose a place to start."
              : error
                ? `Could not update: ${error}.${data ? ` Showing the forecast from ${data.fetchedTime}.` : ""}`
                : data
                  ? `Updated ${data.fetchedTime}: ${Math.round(data.temperature)}°.`
                  : "Waiting for the first forecast…"}
        </span>
        <Button size="bar" disabled={!config.enabled || !config.place} onClick={() => void request("refreshWeather")}>
          Refresh now
        </Button>
      </div>
      <span className="muted small">
        Weather data by{" "}
        <span className="weather-credit">Open-Meteo.com</span>
      </span>
    </div>
  );
}
