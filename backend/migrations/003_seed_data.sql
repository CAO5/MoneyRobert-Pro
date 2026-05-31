INSERT INTO users (username, email, hashed_password, role, is_active, created_at, updated_at)
VALUES (
    'admin',
    'admin@moneyrobert.com',
    '$2b$12$FIyfpirKWOgG5ToQLUjQj.R3s/eVcSdC3fdfLgLrYSPDpCS51TSyi',
    'NORMAL',
    true,
    NOW(),
    NOW()
) ON CONFLICT (username) DO NOTHING;

INSERT INTO users (username, email, hashed_password, role, is_active, created_at, updated_at)
VALUES (
    'demo',
    'demo@moneyrobert.com',
    '$2b$12$FIyfpirKWOgG5ToQLUjQj.R3s/eVcSdC3fdfLgLrYSPDpCS51TSyi',
    'NORMAL',
    true,
    NOW(),
    NOW()
) ON CONFLICT (username) DO NOTHING;
