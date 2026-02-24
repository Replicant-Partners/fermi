Issues:
* AR portal fdifirence in views -
  * from rabble chat no icons and incomplete creature cards
  * from barcode or url input/scan - perfect 
* Location rabbles areas and tethering - lets cleanup the logic and make sure that we have a simple way in a unifieed code base that is clean and sustaonable to maaneg this:
  * creautures ahve locations
  * rabbles have locations (pin) plus area (daius from pin)
  * a host creture location for a rabble is the center pin
  * a host creatre for a rabble canbe thethered - in that case the pinlocaiton of the rabble moves with the host creature
  * a member of a rabble that has a locaiton within the rabble (as dispersed by the logic we wrote before) can opt to follow the rabble if it moves or stay and therefore leave after warining if they wish to stay at their curretn location
  * a tethered creaurer can join a tethered rable as long as their locations stais within the area of the host rabble  - if they fall behind they get awaring you are leaving the rabble and they can choose to remain tethered or follow the rabble (think a running club)
  * when a rabble ends creatures should be atumatically perched at theie last location whre they can join a diffretn rabble or fly somewhere else, or get thethered etc...
  * we just made changes lets make sure this is consitent  - if not lets discuss why
  all of these actions should be tied into notifications -we now have notifications but no way to manage them so we need that bacese im not reciving them - which leads to some more reauiremnts for chat (@ creaure mentions triggering notifications etc, and the blob file suport we were tolaking about - we have apln for ux chat improvments but we dont need to do that until after these other three issues are addressed.)
