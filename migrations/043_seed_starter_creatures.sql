-- Seed 20 starter creatures with real GBIF data (idempotent)
-- Only inserts if no system-owned creatures exist yet

INSERT INTO creatures (creature_id, owner_id, scientific_name, common_name, species_group, gbif_key, taxonomy, specimen_name, asset_path, variation_notes, created_at, updated_at)
SELECT gen_random_uuid(), 'system', v.scientific_name, v.common_name, v.species_group, v.gbif_key, v.taxonomy::jsonb, v.specimen_name, '/static/creatures/placeholder.svg', 'Seed specimen', NOW(), NOW()
FROM (VALUES
  ('Vanessa atalanta', 'Red Admiral', 'butterfly', 1898286, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Nymphalidae","genus":"Vanessa"}', 'Red Admiral #001'),
  ('Papilio machaon', 'Old World Swallowtail', 'butterfly', 1934007, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Papilionidae","genus":"Papilio"}', 'Swallowtail #001'),
  ('Morpho menelaus', 'Blue Morpho', 'butterfly', 1920494, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Nymphalidae","genus":"Morpho"}', 'Blue Morpho #001'),
  ('Danaus plexippus', 'Monarch', 'butterfly', 1920506, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Nymphalidae","genus":"Danaus"}', 'Monarch #001'),
  ('Gonepteryx rhamni', 'Common Brimstone', 'butterfly', 1920671, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Pieridae","genus":"Gonepteryx"}', 'Brimstone #001'),
  ('Aglais io', 'European Peacock', 'butterfly', 1898369, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Nymphalidae","genus":"Aglais"}', 'Peacock #001'),
  ('Pieris brassicae', 'Large White', 'butterfly', 1920290, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Pieridae","genus":"Pieris"}', 'Large White #001'),
  ('Lycaena phlaeas', 'Small Copper', 'butterfly', 1922932, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Lycaenidae","genus":"Lycaena"}', 'Small Copper #001'),
  ('Argynnis paphia', 'Silver-washed Fritillary', 'butterfly', 1898480, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Nymphalidae","genus":"Argynnis"}', 'Fritillary #001'),
  ('Anthocharis cardamines', 'Orange Tip', 'butterfly', 1920756, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Lepidoptera","family":"Pieridae","genus":"Anthocharis"}', 'Orange Tip #001'),
  ('Anax imperator', 'Emperor Dragonfly', 'dragonfly', 1422938, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Aeshnidae","genus":"Anax"}', 'Emperor #001'),
  ('Calopteryx virgo', 'Beautiful Demoiselle', 'dragonfly', 1422191, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Calopterygidae","genus":"Calopteryx"}', 'Demoiselle #001'),
  ('Sympetrum striolatum', 'Common Darter', 'dragonfly', 1423407, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Libellulidae","genus":"Sympetrum"}', 'Darter #001'),
  ('Aeshna cyanea', 'Southern Hawker', 'dragonfly', 1422902, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Aeshnidae","genus":"Aeshna"}', 'Hawker #001'),
  ('Libellula depressa', 'Broad-bodied Chaser', 'dragonfly', 1423222, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Libellulidae","genus":"Libellula"}', 'Chaser #001'),
  ('Ischnura elegans', 'Blue-tailed Damselfly', 'dragonfly', 1422474, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Coenagrionidae","genus":"Ischnura"}', 'Damselfly #001'),
  ('Orthetrum cancellatum', 'Black-tailed Skimmer', 'dragonfly', 1423307, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Libellulidae","genus":"Orthetrum"}', 'Skimmer #001'),
  ('Cordulegaster boltonii', 'Golden-ringed Dragonfly', 'dragonfly', 1423089, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Cordulegastridae","genus":"Cordulegaster"}', 'Golden-ringed #001'),
  ('Pyrrhosoma nymphula', 'Large Red Damselfly', 'dragonfly', 1422405, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Coenagrionidae","genus":"Pyrrhosoma"}', 'Red Damselfly #001'),
  ('Erythromma najas', 'Red-eyed Damselfly', 'dragonfly', 1422443, '{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Odonata","family":"Coenagrionidae","genus":"Erythromma"}', 'Red-eyed #001')
) AS v(scientific_name, common_name, species_group, gbif_key, taxonomy, specimen_name)
WHERE NOT EXISTS (
  SELECT 1 FROM creatures WHERE owner_id = 'system' AND scientific_name = v.scientific_name
);
