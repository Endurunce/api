-- Krachttrainingsdagen per week (0=Ma..6=Zo), optioneel
ALTER TABLE profiles ADD COLUMN IF NOT EXISTS strength_days SMALLINT[] NOT NULL DEFAULT '{}';
