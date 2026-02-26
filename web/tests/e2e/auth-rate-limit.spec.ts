import { expect, test, type APIRequestContext } from '@playwright/test';

const API = 'http://127.0.0.1:8080/api';
const RATE_LIMIT_WINDOW_SECS = Number(process.env.FASTPHOTO_LOGIN_LIMIT_WINDOW_SECS ?? '300');
const MAX_FAILURES = Number(process.env.FASTPHOTO_LOGIN_MAX_FAILURES ?? '10');

interface AuthUser {
  username: string;
  password: string;
}

async function registerUser(request: APIRequestContext, prefix: string): Promise<AuthUser> {
  const username = `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'e2e-pass-123';
  const res = await request.post(`${API}/auth/register`, {
    data: { username, password },
  });
  expect(res.status()).toBe(201);
  return { username, password };
}

test.describe('Auth Rate Limit', () => {
  test.skip(
    RATE_LIMIT_WINDOW_SECS > 30,
    'Set FASTPHOTO_LOGIN_LIMIT_WINDOW_SECS<=30 to run rate-limit recovery E2E quickly'
  );

  test('login rate limit triggers and recovers after window', async ({ request }) => {
    const user = await registerUser(request, 'e2e_rate_limit');
    const wrongPassword = `${user.password}-wrong`;

    for (let i = 0; i < MAX_FAILURES; i += 1) {
      const failed = await request.post(`${API}/auth/login`, {
        data: { username: user.username, password: wrongPassword },
      });
      expect(failed.status()).toBe(401);
    }

    const blockedWrong = await request.post(`${API}/auth/login`, {
      data: { username: user.username, password: wrongPassword },
    });
    expect(blockedWrong.status()).toBe(429);

    const blockedCorrect = await request.post(`${API}/auth/login`, {
      data: { username: user.username, password: user.password },
    });
    expect(blockedCorrect.status()).toBe(429);

    await new Promise((resolve) => setTimeout(resolve, (RATE_LIMIT_WINDOW_SECS + 1) * 1000));

    const recovered = await request.post(`${API}/auth/login`, {
      data: { username: user.username, password: user.password },
    });
    expect(recovered.status()).toBe(200);
    const recoveredBody = await recovered.json();
    expect(recoveredBody.token).toBeTruthy();
    expect(recoveredBody.user?.username).toBe(user.username);
  });
});
