import { describe, it, expect } from "vitest";
import { connectString, isLan } from "./tauri";

describe("isLan", () => {
  it("treats loopback as local", () => {
    expect(isLan({ bind_ip: "127.0.0.1" })).toBe(false);
  });

  it("treats 0.0.0.0 as LAN", () => {
    expect(isLan({ bind_ip: "0.0.0.0" })).toBe(true);
  });
});

describe("connectString", () => {
  it("builds localhost postgres string with stored password", () => {
    expect(
      connectString({
        engine: "postgres",
        port: 5433,
        password: "s3cret!",
        bind_ip: "127.0.0.1",
      }),
    ).toBe("postgres://postgres:s3cret!@127.0.0.1:5433/postgres");
  });

  it("falls back to engine defaults when password is empty", () => {
    expect(
      connectString({ engine: "postgres", port: 5432, password: "", bind_ip: "127.0.0.1" }),
    ).toBe("postgres://postgres:portside@127.0.0.1:5432/postgres");
    expect(
      connectString({ engine: "redis", port: 6379, password: "", bind_ip: "127.0.0.1" }),
    ).toBe("redis://127.0.0.1:6379/0");
  });

  it("adds require-ssl params and rediss scheme for LAN instances", () => {
    expect(
      connectString(
        { engine: "postgres", port: 5433, password: "pw", bind_ip: "0.0.0.0" },
        { host: "192.168.1.10" },
      ),
    ).toBe("postgres://postgres:pw@192.168.1.10:5433/postgres?sslmode=require");
    expect(
      connectString(
        { engine: "redis", port: 6380, password: "pw", bind_ip: "0.0.0.0" },
        { host: "192.168.1.10" },
      ),
    ).toBe("rediss://:pw@192.168.1.10:6380/0");
    expect(
      connectString(
        { engine: "mysql", port: 3307, password: "pw", bind_ip: "0.0.0.0" },
        { host: "192.168.1.10" },
      ),
    ).toBe("mysql://root:pw@192.168.1.10:3307/mysql?ssl-mode=REQUIRED");
  });

  it("uses verify-ca params in strict mode", () => {
    expect(
      connectString(
        { engine: "postgres", port: 5433, password: "pw", bind_ip: "0.0.0.0" },
        { host: "192.168.1.10", strict: true },
      ),
    ).toBe("postgres://postgres:pw@192.168.1.10:5433/postgres?sslmode=verify-ca");
  });
});
