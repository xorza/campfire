use crate::zero_hour::format::ini::object_ini::{ObjectFiles, ObjectIni};

/// `Data\INI\Default\Object.ini` of the fixture install: a tank of two draw modules, one of
/// which draws no model and one of which names two animations and a transition between two
/// keys, and an upgrade that shows its turret and hides its flash; and a tree.
pub(crate) const DEFAULT_OBJECTS: &str = "; Objects the game loads first.\r
Object Tank\r
  Draw = W3DTankDraw ModuleTag_01\r
DefaultConditionState\r
  Model = TANK\r
  HideSubObject = Turret\r
  ShowSubObject = Barrel\r
  HideSubObject = barrel Hatch\r
  WeaponMuzzleFlash = PRIMARY MuzzleFX\r
  WeaponLaunchBone = PRIMARY None\r
  WeaponFireFXBone = SECONDARY Fire ; a slot's bones need not name a flash\r
  Turret = Turret\r
  Animation = TANK.TANK_AIM\r
  TransitionKey = Rest\r
End\r
ConditionState = REALLYDAMAGED\r
  Model = TANK_D\r
  IdleAnimation = Ghost.Ghost_Walk\r
  Flags = ADJUST_HEIGHT_BY_CONSTRUCTION_PERCENT\r
End\r
ConditionState = NIGHT\r
  ShowSubObject = Turret\r
  TransitionKey = Lit-Up\r
End\r
TransitionState = Rest Lit-Up\r
  AnimationMode = ONCE_BACKWARDS\r
End\r
  End\r
  Draw = W3DDebrisDraw ModuleTag_02\r
Model = Debris\r
  End\r
  Behavior = SlowDeathBehavior ModuleTag_03\r
Turret\r
End\r
  End\r
  Behavior = SubObjectsUpgrade ModuleTag_04\r
TriggeredBy = Upgrade_AmericaCompositeArmor\r
ShowSubObjects = Turret\r
HideSubObjects = Flash\r
  End\r
  ArmorSet\r
Conditions = None\r
  End\r
End\r
\r
Object Tree\r
  Draw W3DTreeDraw ModuleTag_01\r
ModelName PTOak01\r
  End\r
End\r
";

/// `Data\INI\Object\Misc.ini` of the fixture install: objects of models that draw nothing,
/// of an emitter, and of no file.
pub(crate) const MORE_OBJECTS: &str = "Object Crate
  Draw = W3DModelDraw ModuleTag_01
DefaultConditionState
  Model = Prop
End
  End
End
Object Marker
  Draw = W3DModelDraw ModuleTag_01
DefaultConditionState
  Model = NULL
End
  End
End
Object Hydrant
  Draw = W3DModelDraw ModuleTag_01
DefaultConditionState
  Model = Spray
End
  End
End
Object Ghost
  KindOf = PROP
  Draw = W3DModelDraw ModuleTag_01
DefaultConditionState
  Model = Absent
End
  End
End
";

/// `Data\INI\Object.ini` of the fixture install: two reskins of the tank, one with its own
/// draw module, a pair whose build variations they are, a stump of no model, a prop of a lone mesh at a scale of 1.5 and its
/// reskin, an optimized tree, fluff, a wall a unit walks on, a structure of a box of half
/// extents 10 and 2.5, and the fixture map's tank, which drives on its treads, a box of half
/// extents 4 and 3, its gun on a turret of 180° a second, and its fence; the shell the tank's
/// cannon fires, a sphere of radius 1; and America's start objects, which no map places: a
/// command center of a box of half extents 12 and 9 and height 40, whose sphere's radius is
/// √(144 + 81 + 400) = 25, and a dozer on treads.
pub(crate) const OBJECTS: &str = "ObjectReskin TankCamo Tank\r
End\r
ObjectReskin TankRed Tank\r
  Draw = W3DModelDraw ModuleTag_01\r
ConditionState NONE\r
  Model = TANK\r
  HideSubObject = Barrel\r
  ShowSubObject = None\r
  ShowSubObject = Hatch\r
End\r
  End\r
End\r
Object TankPair\r
  BuildVariations = TankCamo TankRed\r
End\r
Object Stump\r
  Draw = W3DPropDraw ModuleTag_01\r
ModelName = NONE\r
  End\r
End\r
Object Rock\r
  KindOf = PROP\r
  Scale = 1.5\r
  Draw = W3DModelDraw ModuleTag_01\r
ConditionState\r
  Model = Rock01\r
End\r
  End\r
End\r
ObjectReskin RockRed Rock\r
  Scale = 3.0\r
End\r
Object Oak\r
  KindOf = OPTIMIZED_TREE\r
  Draw = W3DModelDraw ModuleTag_01\r
DefaultConditionState\r
  Model = TANK\r
End\r
  End\r
  Draw = W3DTreeDraw ModuleTag_02\r
ModelName = Rock01\r
  End\r
End\r
Object Bush\r
  KindOf = CLEARED_BY_BUILD\r
  Draw = W3DModelDraw ModuleTag_01\r
DefaultConditionState\r
  Model = TANK\r
  HideSubObject = Turret\r
End\r
  End\r
End\r
Object AmericaTankCrusader\r
  KindOf = VEHICLE\r
  Body = ActiveBody ModuleTag_04\r
MaxHealth = 400.0\r
InitialHealth = 400.0\r
  End\r
  WeaponSet\r
Conditions = None\r
Weapon = PRIMARY TankCannon\r
Weapon = SECONDARY RangerRifle\r
Weapon = TERTIARY Railgun\r
  End\r
  WeaponSet\r
Conditions = CRATEUPGRADE_ONE\r
Weapon = PRIMARY TankCannon\r
  End\r
  ArmorSet\r
Conditions = None\r
Armor = HumanArmor\r
  End\r
  ArmorSet\r
Conditions = PLAYER_UPGRADE\r
Armor = StructureArmor\r
  End\r
  Behavior = ArmorUpgrade ModuleTag_20\r
TriggeredBy = Upgrade_AmericaCompositeArmor\r
  End\r
  Behavior = MaxHealthUpgrade ModuleTag_21\r
TriggeredBy = Upgrade_AmericaCompositeArmor\r
AddMaxHealth = 100.0\r
ChangeType = ADD_CURRENT_HEALTH_TOO\r
  End\r
  Behavior = MaxHealthUpgrade ModuleTag_22\r
TriggeredBy = Upgrade_AmericaCompositeArmor\r
AddMaxHealth = 50.0\r
ChangeType = PRESERVE_RATIO\r
  End\r
  Behavior = StealthUpgrade ModuleTag_23\r
TriggeredBy = Upgrade_AmericaCompositeArmor\r
  End\r
  Locomotor = SET_NORMAL TankTreads\r
  Behavior = AIUpdateInterface ModuleTag_06\r
Turret\r
  TurretTurnRate = 180\r
  ControlledWeaponSlots = PRIMARY\r
End\r
AutoAcquireEnemiesWhenIdle = Yes\r
MoodAttackCheckRate = 250\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 4.0\r
  GeometryMinorRadius = 3.0\r
End\r
Object TankShell\r
  KindOf = PROJECTILE\r
  Body = ActiveBody ModuleTag_02\r
MaxHealth = 100.0\r
  End\r
  Behavior = DumbProjectileBehavior ModuleTag_03\r
FirstHeight = 10\r
SecondHeight = 10\r
FirstPercentIndent = 50%\r
SecondPercentIndent = 90%\r
FlightPathAdjustDistPerSecond = 100\r
  End\r
  Behavior = WeaponBonusUpgrade ModuleTag_04\r
TriggeredBy = Upgrade_AmericaCompositeArmor\r
  End\r
  Geometry = SPHERE\r
  GeometryMajorRadius = 1.0\r
End\r
Object FortressWall\r
  KindOf = WALK_ON_TOP_OF_WALL IMMOBILE STRUCTURE\r
  Body = HighlanderBody ModuleTag_05\r
MaxHealth = 1000.0\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 10.0\r
  GeometryMinorRadius = 2.5\r
End\r
Object Rocks1\r
  KindOf = CLEARED_BY_BUILD\r
  FenceWidth = 10.0\r
End\r
Object AmericaCommandCenter\r
  KindOf = STRUCTURE MP_COUNT_FOR_VICTORY\r
  Body = StructureBody ModuleTag_08\r
MaxHealth = 5000.0\r
InitialHealth = 5000.0\r
  End\r
  ArmorSet\r
Conditions = None\r
Armor = StructureArmor\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 12.0\r
  GeometryMinorRadius = 9.0\r
  GeometryHeight = 40.0\r
  BuildCost = 2000\r
  BuildTime = 45.0\r
  CommandSet = AmericaCommandCenterCommandSet\r
  PlacementViewAngle = -135\r
  Behavior = ProductionUpdate ModuleTag_11\r
  End\r
  Behavior = DefaultProductionExitUpdate ModuleTag_13\r
UnitCreatePoint = X:-18.0 Y:35.0 Z:0.0\r
NaturalRallyPoint = X:60.0 Y:35.0 Z:0.0\r
  End\r
End\r
Object AmericaVehicleDozer\r
  KindOf = VEHICLE\r
  Body = ActiveBody ModuleTag_02\r
MaxHealth = 250.0\r
InitialHealth = 200.0\r
  End\r
  Locomotor = SET_NORMAL TankTreads\r
  Behavior = DozerAIUpdate ModuleTag_03\r
AutoAcquireEnemiesWhenIdle = Yes\r
  End\r
  Geometry = SPHERE\r
  GeometryMajorRadius = 2.0\r
  BuildCost = 1000\r
  BuildTime = 5.0\r
  CommandSet = AmericaDozerCommandSet\r
End\r
Object AmericaBarracks\r
  KindOf = STRUCTURE\r
  Body = StructureBody ModuleTag_09\r
MaxHealth = 1200.0\r
InitialHealth = 1200.0\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 5.0\r
  GeometryMinorRadius = 4.0\r
  BuildCost = 600\r
  BuildTime = 10.0\r
  CommandSet = AmericaBarracksCommandSet\r
  Prerequisites\r
  End\r
  Behavior = ProductionUpdate ModuleTag_11\r
MaxQueueEntries = 5\r
  End\r
  Behavior = MaxHealthUpgrade ModuleTag_20\r
TriggeredBy = Upgrade_AmericaBarracksWalls\r
AddMaxHealth = 200.0\r
ChangeType = ADD_CURRENT_HEALTH_TOO\r
  End\r
  Behavior = QueueProductionExitUpdate ModuleTag_12\r
UnitCreatePoint = X:0.0 Y:0.0 Z:0.0\r
NaturalRallyPoint = X:55.0 Y:0.0 Z:0.0\r
  End\r
End\r
Object AmericaInfantryRanger\r
  KindOf = INFANTRY\r
  Body = ActiveBody ModuleTag_03\r
MaxHealth = 180.0\r
InitialHealth = 180.0\r
  End\r
  WeaponSet\r
Conditions = PLAYER_UPGRADE\r
Weapon = PRIMARY RangerRifle\r
Weapon = SECONDARY Bolt\r
Weapon = TERTIARY Flamer\r
  End\r
  Behavior = WeaponSetUpgrade ModuleTag_20\r
TriggeredBy = Upgrade_AmericaRangerFlashBang\r
  End\r
  Behavior = WeaponBonusUpgrade ModuleTag_21\r
TriggeredBy = Upgrade_AmericaRangerFlashBang Upgrade_AmericaCompositeArmor\r
RequiresAllTriggers = Yes\r
ConflictsWith = Upgrade_AmericaBarracksWalls Upgrade_Unknown\r
  End\r
  Behavior = ArmorUpgrade ModuleTag_22\r
TriggeredBy = Upgrade_Lost\r
  End\r
  WeaponSet\r
Conditions = None\r
Weapon = PRIMARY RangerRifle\r
Weapon = SECONDARY Pistol\r
Weapon = TERTIARY NONE\r
  End\r
  ArmorSet\r
Conditions = None\r
Armor = HumanArmor\r
  End\r
  Locomotor = SET_NORMAL TankTreads\r
  Geometry = CYLINDER\r
  GeometryMajorRadius = 1.0\r
  BuildCost = 225\r
  BuildTime = 5.0\r
  Prerequisites\r
Object = AmericaBarracks\r
  End\r
End\r
Object AmericaInfantryPathfinder\r
  KindOf = INFANTRY\r
  Locomotor = SET_NORMAL TankTreads\r
  BuildCost = 300\r
  BuildTime = 6.0\r
  Prerequisites\r
Object = AmericaBarracks AmericaCommandCenter\r
  End\r
End\r
Object AmericaPowerPlant\r
  KindOf = STRUCTURE\r
  Geometry = BOX\r
  GeometryMajorRadius = 3.0\r
  GeometryMinorRadius = 3.0\r
  Prerequisites\r
Science = SCIENCE_AMERICA\r
  End\r
End\r
Object AmericaFireBase\r
  KindOf = STRUCTURE\r
  GeometryIsSmall = Yes\r
End\r
";

impl ObjectIni {
    /// The objects of `files`, read in their order, as an install's: a test's.
    pub(crate) fn of(files: &[&str]) -> Self {
        let mut read = ObjectFiles::default();
        for file in files {
            read.read(file.as_bytes()).unwrap();
        }
        read.finish()
    }
}
