# Measurements available to vitals-mcp

Source: a **Polar H10** chest strap. It streams to the Android app [PolarH10](../PolarH10), which publishes over MQTT to [PolarH10-server](../PolarH10-server). That server stores everything in **PostgreSQL + TimescaleDB**. vitals-mcp only needs to read from that database.

- Connection: `postgres://polar:<POSTGRES_PASSWORD>@<host>:5432/polar`. Database and user are both `polar`; the password is in `PolarH10-server/.env`.
  - The compose file doesn't publish 5432 on the host. Add a port mapping, or run vitals-mcp in the same compose network as host `db`.
- One user, one strap. `device_id` is the strap's ID, e.g. `244E7234`.
- Recording runs 24/7 while the strap is worn and in Bluetooth range of the phone.
- All tables are TimescaleDB hypertables with 1-day chunks. Data older than 7 days is compressed (still queryable, just slower to scan raw).

## Tables

Each row is one **frame** as the strap sent it, not one sample. ECG and accelerometer samples are stored as arrays.

| Table | Rate | Columns | Units / meaning |
|---|---|---|---|
| `hr` | ~1 row/s | `time`, `device_id`, `hr int`, `rr_ms int[]`, `contact bool` | Heart rate in bpm. `rr_ms` = beat-to-beat (RR) intervals in ms received in that second (0–4 values). `contact` = skin contact. |
| `ecg` | ~1.8 rows/s, ~73 samples each | `time`, `device_id`, `sample_rate int`, `samples int[]` | Single-lead ECG in **µV**, 130 Hz. |
| `acc` | ~5.5 rows/s, ~36 samples each | `time`, `device_id`, `sample_rate int`, `x int[]`, `y int[]`, `z int[]` | Chest accelerometer in **milli-g** (1000 ≈ 1 g), 200 Hz. |
| `battery` | on connect / change | `time`, `device_id`, `level int` | Strap battery %. |
| `status` | on events | `time`, `device_id`, `event text` | `connected`, `disconnected`, `stopped`, `clock_skew`. |

Every table has `UNIQUE (device_id, time)`, so a repeated `(device_id, time)` is ignored.

### Timing

- `time` is the timestamp of the **first** sample in the frame, at µs precision.
  - Sample `i` is at `time + i / sample_rate` seconds (index 0-based; Postgres arrays are 1-based, so use `i - 1`).
- **ECG and ACC:** timed by the strap's own clock, which the app sets to UTC on every connect. They're accurate to the millisecond.
- **HR:** timed by the phone when the reading arrives (the H10 doesn't timestamp HR). It's a fraction of a second behind ECG.
- The last value in `rr_ms` ends at `time`, and earlier values come before it.

## Gotchas (important for correct answers)

1. **Filter on skin contact.** With the strap off, the H10 still sends plausible-looking garbage, e.g. a flat 164 bpm with random RR values, marked `contact = false`. Always use `WHERE contact IS NOT FALSE`. `NULL` means the strap didn't report contact, so treat it as valid.
2. **Gaps are normal.** They happen when the strap is off, the phone is out of range, or the strap is charging. Use `status` events and gaps in `time` to tell "not recorded" apart from "low value". Don't interpolate across gaps longer than a few seconds.
3. **RR intervals are the basis for HRV.** Compute RMSSD or SDNN from `rr_ms`, not from `hr`.
   - Drop physiologically impossible intervals (`< 300` or `> 2000` ms).
   - Drop successive differences larger than ~20 %. These are ectopic beats or artefacts, common during movement.
4. **ECG and ACC are high-volume.** That's about 11 k ECG and 17 k ACC samples per minute. Aggregate in SQL (`time_bucket`), and never return raw arrays for long periods.
5. Timestamps are `timestamptz`, so convert to the user's local time zone when presenting.

## Example queries

```sql
-- Heart rate per minute, last 24 h (valid readings only)
SELECT time_bucket('1 minute', time) AS minute,
       round(avg(hr)) AS avg_bpm, min(hr) AS min_bpm, max(hr) AS max_bpm
FROM hr
WHERE time > now() - interval '24 hours' AND contact IS NOT FALSE AND hr > 0
GROUP BY 1 ORDER BY 1;

-- HRV (RMSSD, ms) per 5 minutes, from cleaned RR intervals
WITH rr AS (
  SELECT h.time, r.rr, r.ord
  FROM hr h, unnest(h.rr_ms) WITH ORDINALITY AS r(rr, ord)
  WHERE h.time > now() - interval '24 hours' AND h.contact IS NOT FALSE
    AND r.rr BETWEEN 300 AND 2000
), d AS (
  SELECT time, rr - lag(rr) OVER (ORDER BY time, ord) AS diff, rr
  FROM rr
)
SELECT time_bucket('5 minutes', time) AS bucket,
       round(sqrt(avg(diff ^ 2))::numeric, 1) AS rmssd_ms,
       count(*) AS beats
FROM d
WHERE abs(diff) < 0.2 * rr
GROUP BY 1 HAVING count(*) > 100 ORDER BY 1;

-- Resting heart rate per day: lowest 10-minute average
SELECT day, min(avg_bpm) AS resting_bpm FROM (
  SELECT time_bucket('1 day', time) AS day, time_bucket('10 minutes', time) AS b, avg(hr) AS avg_bpm
  FROM hr WHERE contact IS NOT FALSE AND hr > 0
  GROUP BY 1, 2 HAVING count(*) > 300
) t GROUP BY day ORDER BY day;

-- Activity per minute: mean deviation of acceleration magnitude from 1 g (mg)
SELECT time_bucket('1 minute', a.time) AS minute,
       round(avg(abs(sqrt(sx*sx + sy*sy + sz*sz) - 1000))) AS activity_mg
FROM acc a, unnest(a.x, a.y, a.z) AS s(sx, sy, sz)
WHERE a.time > now() - interval '1 hour'
GROUP BY 1 ORDER BY 1;

-- When was the strap worn? Continuous HR coverage per hour
SELECT time_bucket('1 hour', time) AS hour,
       round(100.0 * count(*) FILTER (WHERE contact IS NOT FALSE) / 3600, 1) AS pct_worn
FROM hr WHERE time > now() - interval '7 days'
GROUP BY 1 ORDER BY 1;

-- Raw ECG for a short window (keep it to seconds), one row per sample
SELECT e.time + (i - 1) * interval '1 second' / e.sample_rate AS t, e.samples[i] AS uv
FROM ecg e, generate_subscripts(e.samples, 1) AS i
WHERE e.time BETWEEN '2026-11-04 18:00:00+01' AND '2026-11-04 18:00:10+01'
ORDER BY t;
```

## Suggested MCP tools

- `heart_rate(from, to, bucket)`: avg, min and max bpm per bucket, with contact filtering.
- `hrv(from, to, bucket)`: RMSSD and SDNN from cleaned RR intervals.
- `resting_hr(days)`, `activity(from, to, bucket)`, `wear_time(from, to)`.
- `ecg_snippet(at, seconds ≤ 30)`: raw µV samples for inspecting one event.
- `strap_status()`: latest battery and connection events, and the time of the last data.
