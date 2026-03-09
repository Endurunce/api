-- Krachttrainingsdagen per week (0=Ma..6=Zo), optioneel
ALTER TABLE profiles ADD COLUMN strength_days SMALLINT[] NOT NULL DEFAULT '{}';
