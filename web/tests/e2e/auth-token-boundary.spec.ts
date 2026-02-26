import { expect, test, type APIRequestContext } from '@playwright/test';

const API = 'http://127.0.0.1:8080/api';

interface AuthUser {
  username: string;
  password: string;
  token: string;
}

async function registerUser(request: APIRequestContext, prefix: string): Promise<AuthUser> {
  const username = `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'e2e-pass-123';
  const res = await request.post(`${API}/auth/register`, {
    data: { username, password },
  });
  expect(res.status()).toBe(201);
  const body = await res.json();
  return { username, password, token: body.token as string };
}

test.describe('Auth Token Boundary', () => {
  test('protected endpoint rejects missing/malformed/invalid token and accepts valid token', async ({
    request,
  }) => {
    const user = await registerUser(request, 'e2e_token_boundary');

    const noHeader = await request.get(`${API}/auth/me`);
    expect(noHeader.status()).toBe(401);

    const malformedHeader = await request.get(`${API}/auth/me`, {
      headers: { Authorization: 'Token abc' },
    });
    expect(malformedHeader.status()).toBe(401);

    const invalidBearer = await request.get(`${API}/auth/me`, {
      headers: { Authorization: 'Bearer not-a-jwt-token' },
    });
    expect(invalidBearer.status()).toBe(401);

    const valid = await request.get(`${API}/auth/me`, {
      headers: { Authorization: `Bearer ${user.token}` },
    });
    expect(valid.status()).toBe(200);
    const validBody = await valid.json();
    expect(validBody.username).toBe(user.username);

    const timelineInvalid = await request.get(`${API}/photos/timeline`, {
      headers: { Authorization: 'Bearer definitely-invalid' },
    });
    expect(timelineInvalid.status()).toBe(401);
  });
});
