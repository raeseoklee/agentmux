import { describe, expect, it } from "vitest";
import {
  AUTO_UPDATE_PERIODIC_INTERVAL_MS,
  AUTO_UPDATE_RESUME_STALE_MS,
  isAutomaticUpdateCheckDue,
  shouldPauseAutomaticUpdateChecks,
} from "./updateCheckSchedule";

describe("automatic update check schedule", () => {
  it("checks immediately when no previous attempt exists", () => {
    expect(
      isAutomaticUpdateCheckDue(null, 1_000, AUTO_UPDATE_RESUME_STALE_MS),
    ).toBe(true);
  });

  it("waits until the requested freshness interval has elapsed", () => {
    const lastAttemptAt = 10_000;
    expect(
      isAutomaticUpdateCheckDue(
        lastAttemptAt,
        lastAttemptAt + AUTO_UPDATE_RESUME_STALE_MS - 1,
        AUTO_UPDATE_RESUME_STALE_MS,
      ),
    ).toBe(false);
    expect(
      isAutomaticUpdateCheckDue(
        lastAttemptAt,
        lastAttemptAt + AUTO_UPDATE_RESUME_STALE_MS,
        AUTO_UPDATE_RESUME_STALE_MS,
      ),
    ).toBe(true);
  });

  it("uses a longer cadence for unattended periodic checks", () => {
    expect(AUTO_UPDATE_PERIODIC_INTERVAL_MS).toBeGreaterThan(
      AUTO_UPDATE_RESUME_STALE_MS,
    );
  });

  it("recovers when the system clock moves backwards", () => {
    expect(
      isAutomaticUpdateCheckDue(20_000, 10_000, AUTO_UPDATE_RESUME_STALE_MS),
    ).toBe(true);
  });

  it("pauses while an update resource is available or being installed", () => {
    expect(shouldPauseAutomaticUpdateChecks("not_available", false)).toBe(
      false,
    );
    expect(shouldPauseAutomaticUpdateChecks("not_available", true)).toBe(true);
    expect(shouldPauseAutomaticUpdateChecks("available", false)).toBe(true);
    expect(shouldPauseAutomaticUpdateChecks("downloading", false)).toBe(true);
    expect(shouldPauseAutomaticUpdateChecks("installed", false)).toBe(true);
  });
});
